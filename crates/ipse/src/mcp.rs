//! `ipse mcp`: serves the memory to an AI app over MCP (`docs/spec/rights.md`, sections
//! 3 and 4), on standard input and output, or over Streamable HTTP on this computer
//! (`http`).
//!
//! On standard input and output, the AI app starts this command itself and passes the
//! access code in `IPSE_TOKEN`; over HTTP, it sends the code with each request. Every
//! tool goes through [`Gateway::fresh_access`]: the code is checked again on each call,
//! before anything is read, so that removing an AI or changing its rights applies from
//! its next request.

mod http;

use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};

use ipse_core::Error as IpseError;
use ipse_core::rights::{Access, Gateway, MAX_EVENTS, MAX_STATES};
use ipse_core::search::{DEFAULT_LIMIT, MAX_LIMIT};
use ipse_core::vault::EventTime;
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, ContentBlock, Implementation, ServerCapabilities, ServerConfig};
use rmcp::service::RequestContext;
use rmcp::{RoleServer, ServerHandler, ServiceExt, schemars, tool, tool_handler, tool_router};
use serde::Deserialize;
use serde_json::{Value, json};

/// The environment variable holding the access code.
const TOKEN_VAR: &str = "IPSE_TOKEN";

/// Most entities `get` returns for one name.
const MAX_NAMED: usize = 10;

const INSTRUCTIONS: &str = "Ipse holds the memory of the person you are talking with: the \
people, places, events and things in their life, imported from their apps or written by them. \
Call `about_me` to know who they are. When they mention someone or something you do not know, \
`search` it, then `get` it in full. For their agenda (\"what do I have tomorrow?\", \"when did \
I last see Marie?\"), use `events`. For what is going on in their life right now (music, \
series, books), use `now`; `states_history` is only for questions about a past period. You only \
see what the person allowed you to see.";

/// Serves the memory in `vault` (default: the current folder) until the AI app closes
/// the connection; with `http`, over HTTP at that address until stopped.
pub(crate) fn run(vault: Option<PathBuf>, http: Option<&str>) -> ipse_core::Result<()> {
    let root = vault.unwrap_or_else(|| PathBuf::from("."));
    if let Some(address) = http {
        let address = http::address(address)?;
        return http::serve(Gateway::open(&root)?, address);
    }
    let gateway = Gateway::open(&root)?;
    let token = std::env::var(TOKEN_VAR)
        .ok()
        .map(|t| t.trim().to_owned())
        .filter(|t| !t.is_empty());
    // The AI app shows standard error in its logs: say why every request will fail.
    match &token {
        None => eprintln!("{}", no_token()),
        Some(t) if gateway.access(Some(t)).is_err() => eprintln!(
            "{TOKEN_VAR} is not a valid access code: every request will be refused. \
             Connect the AI again with `ipse client add`."
        ),
        Some(_) => {}
    }

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| stopped(&e))?;
    runtime.block_on(async {
        let service = Server::new(Arc::new(Mutex::new(gateway)), Token::Fixed(token))
            .serve(rmcp::transport::stdio())
            .await
            .map_err(|e| stopped(&e))?;
        service.waiting().await.map_err(|e| stopped(&e))?;
        Ok(())
    })
}

fn stopped(error: &dyn std::fmt::Display) -> IpseError {
    IpseError::Refused(format!("The connection with the AI app stopped: {error}"))
}

fn no_token() -> String {
    format!(
        "No access code: set {TOKEN_VAR} in this AI app's settings for Ipse. \
         `ipse client add` gives one."
    )
}

/// Where the access code comes from.
#[derive(Debug, Clone)]
enum Token {
    /// Given once, when the AI app started `ipse mcp` (standard input and output).
    Fixed(Option<String>),
    /// Sent with each HTTP request, as `Authorization: Bearer <code>`.
    PerRequest,
}

/// The MCP server: the vault's gateway and where the access code comes from.
struct Server {
    gateway: Arc<Mutex<Gateway>>,
    token: Token,
    tool_router: ToolRouter<Self>,
}

/// Arguments of `search`.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
struct SearchRequest {
    /// Words to look for: a name, part of a name, or words of a fact. Accents and case
    /// do not matter.
    query: String,
    /// How many results at most: 10 when not given, 50 at most.
    #[serde(default)]
    limit: Option<usize>,
}

/// Arguments of `get`: one of `id` or `name`.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
struct GetRequest {
    /// The id of an entity, as `search` returns it.
    #[serde(default)]
    id: Option<String>,
    /// A name or other name, like "Marie Dupont", or the target of a link, like
    /// "[[marie-dupont]]". Accents and case do not matter.
    #[serde(default)]
    name: Option<String>,
}

/// Arguments of `events`.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
struct EventsRequest {
    /// First day of the period, like "2026-11-14", or a date and time with its offset.
    from: String,
    /// Last day of the period, included, like "2026-11-20".
    to: String,
    /// Only the events this person (or entity) takes part in: an id from `search`.
    #[serde(default)]
    with: Option<String>,
    /// How many events at most: 200 when not given, 200 at most.
    #[serde(default)]
    limit: Option<usize>,
}

/// Arguments of `now`.
#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
struct NowRequest {
    /// Topics to look at, like `["music", "watching"]`; every topic when not given.
    #[serde(default)]
    topics: Vec<String>,
}

/// Arguments of `states_history`.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
struct StatesHistoryRequest {
    /// The topic: "music", "watching", "reading", "location", "home"…
    topic: String,
    /// First day of the period, like "2026-06-01".
    from: String,
    /// Last day of the period, included, like "2026-08-31".
    to: String,
    /// How many states at most: 200 when not given, 200 at most.
    #[serde(default)]
    limit: Option<usize>,
}

#[tool_router]
#[allow(clippy::needless_pass_by_value)] // rmcp passes what a tool takes by value
impl Server {
    fn new(gateway: Arc<Mutex<Gateway>>, token: Token) -> Self {
        Self {
            gateway,
            token,
            tool_router: Self::tool_router() + Self::time_router(),
        }
    }

    #[tool(
        description = "Who the person you are talking with is: their name, birthday, contact \
details and what they wrote about themselves. Call it once at the start of a conversation. \
Example: about_me {}. Returns {\"me\": {...}}, or {\"me\": null} when there is nothing to show."
    )]
    fn about_me(&self, context: RequestContext<RoleServer>) -> CallToolResult {
        self.read(&context, |access| json!({ "me": access.about_me() }))
    }

    #[tool(
        description = "Find people, places and other entities in the person's memory by words: \
a name, part of a name, or words from what is known about them. Results come best first, each \
with its id, type, name and a short excerpt; `more` is true when there were more than `limit`. \
Example: when the person mentions \"Marie\", call search {\"query\": \"Marie\"}, then get the \
right result by its id."
    )]
    fn search(
        &self,
        Parameters(request): Parameters<SearchRequest>,
        context: RequestContext<RoleServer>,
    ) -> CallToolResult {
        let limit = request.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
        self.read(&context, |access| {
            let found = access.search(&request.query, limit);
            let results: Vec<Value> = found
                .hits
                .iter()
                .map(|hit| {
                    json!({
                        "id": hit.id.to_string(),
                        "type": hit.file_type.as_str(),
                        "name": hit.name,
                        "excerpt": hit.excerpt,
                    })
                })
                .collect();
            json!({ "results": results, "more": found.more })
        })
    }

    #[tool(
        description = "Everything the person's memory holds about one entity: names, birthday, \
contact details, facts, relations and notes. Give the `id` of a search result, or a `name`. \
Several entities come back when several share that name: ask the person which one they mean. \
A relation's target, like \"[[paul-durand]]\", can be passed as `name`. Examples: \
get {\"id\": \"01jd5z8x3k4m2n6p7q9r0s1t2v\"}, get {\"name\": \"Marie Dupont\"}."
    )]
    fn get(
        &self,
        Parameters(request): Parameters<GetRequest>,
        context: RequestContext<RoleServer>,
    ) -> CallToolResult {
        let id = request
            .id
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty());
        let name = request
            .name
            .as_deref()
            .map(link_target)
            .filter(|s| !s.is_empty());
        match (id, name) {
            (Some(id), None) => self.read(&context, |access| {
                let entries: Vec<_> = access.get(id).into_iter().collect();
                json!({ "entries": entries, "more": false })
            }),
            (None, Some(name)) => self.read(&context, |access| {
                let mut entries = access.get_by_name(name);
                let more = entries.len() > MAX_NAMED;
                entries.truncate(MAX_NAMED);
                json!({ "entries": entries, "more": more })
            }),
            _ => refusal("Give either an `id` or a `name`, like get {\"name\": \"Marie Dupont\"}."),
        }
    }
}

#[tool_router(router = time_router)]
#[allow(clippy::needless_pass_by_value)] // rmcp passes what a tool takes by value
impl Server {
    #[tool(
        description = "The person's events in a period, ordered by start: title, start, end, place, \
participants (links to people, or names) and notes. Days count whole: from and to are included. \
Give `with` (an id from `search`) for the events someone takes part in. At most 200; `more` is \
true when there were more. Examples: events {\"from\": \"2026-11-14\", \"to\": \"2026-11-14\"} \
for \"what do I have tomorrow?\"; events {\"from\": \"2025-11-14\", \"to\": \"2026-11-14\", \
\"with\": \"01jd5z8x3k4m2n6p7q9r0s1t2v\"} for \"when did I last see Marie?\"."
    )]
    fn events(
        &self,
        Parameters(request): Parameters<EventsRequest>,
        context: RequestContext<RoleServer>,
    ) -> CallToolResult {
        let Some((from, to)) = period(&request.from, &request.to) else {
            return refusal(PERIOD);
        };
        let limit = request.limit.unwrap_or(MAX_EVENTS);
        self.read(&context, |access| {
            let found = access.events(&from, &to, request.with.as_deref(), limit);
            json!({ "events": found.events, "more": found.more })
        })
    }

    #[tool(
        description = "What is going on in the person's life right now: the music they listen to, \
the series they watch, the books they read, where they are… Only states that are current, each \
with when it was last seen. Example: now {} or now {\"topics\": [\"music\"]}."
    )]
    fn now(
        &self,
        Parameters(request): Parameters<NowRequest>,
        context: RequestContext<RoleServer>,
    ) -> CallToolResult {
        self.read(&context, |access| {
            let found = access.now(&request.topics);
            json!({ "states": found.states, "more": found.more })
        })
    }

    #[tool(
        description = "What the person listened to, watched, read… during a past period, each state \
with its status: current, recent or past. Use it only when they ask about a period (\"what was I \
listening to last summer?\"); for now, use `now`. Example: states_history {\"topic\": \"music\", \
\"from\": \"2026-06-01\", \"to\": \"2026-08-31\"}."
    )]
    fn states_history(
        &self,
        Parameters(request): Parameters<StatesHistoryRequest>,
        context: RequestContext<RoleServer>,
    ) -> CallToolResult {
        let Some((from, to)) = period(&request.from, &request.to) else {
            return refusal(PERIOD);
        };
        let limit = request.limit.unwrap_or(MAX_STATES);
        self.read(&context, |access| {
            let found = access.states_history(&request.topic, &from, &to, limit);
            json!({ "states": found.states, "more": found.more })
        })
    }
}

const PERIOD: &str = "Give `from` and `to` as days like \"2026-11-14\" (or a date and time \
with its offset), `from` not after `to`.";

/// `from` and `to` as event times, `from` not after `to`.
fn period(from: &str, to: &str) -> Option<(EventTime, EventTime)> {
    let from = EventTime::parse(from.trim())?;
    let to = EventTime::parse(to.trim())?;
    (from.date() <= to.date()).then_some((from, to))
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for Server {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(
                Implementation::new("ipse", env!("CARGO_PKG_VERSION")).with_title("Ipse"),
            )
            .with_instructions(INSTRUCTIONS)
    }
}

impl Server {
    /// Checks the access code, then runs `query` with what it allows.
    fn read(
        &self,
        context: &RequestContext<RoleServer>,
        query: impl FnOnce(&Access<'_>) -> Value,
    ) -> CallToolResult {
        let token = match &self.token {
            Token::Fixed(token) => token.clone(),
            Token::PerRequest => context
                .extensions
                .get::<hyper::http::request::Parts>()
                .and_then(|parts| http::bearer(&parts.headers))
                .map(str::to_owned),
        };
        let Some(token) = token else {
            return refusal(&no_token());
        };
        let mut gateway = self.gateway.lock().unwrap_or_else(PoisonError::into_inner);
        match gateway.fresh_access(Some(&token)) {
            Ok(access) => CallToolResult::structured(query(&access)),
            Err(IpseError::Refused(message)) => refusal(&message),
            Err(error) => {
                // The details name files of the vault: they stay on this computer.
                eprintln!("{error}");
                refusal("Ipse could not read the memory folder. Its logs say why.")
            }
        }
    }
}

fn refusal(message: &str) -> CallToolResult {
    CallToolResult::error(vec![ContentBlock::text(message)])
}

/// `name` without the `[[ ]]` and display text of a link: `[[marie-dupont|Marie]]`
/// gives `marie-dupont`.
fn link_target(name: &str) -> &str {
    let name = name.trim();
    match name.strip_prefix("[[").and_then(|n| n.strip_suffix("]]")) {
        Some(inner) => inner.split('|').next().unwrap_or_default().trim(),
        None => name,
    }
}

#[cfg(test)]
mod tests {
    use super::link_target;

    #[test]
    fn link_targets_are_names() {
        assert_eq!(link_target(" Marie Dupont "), "Marie Dupont");
        assert_eq!(link_target("[[marie-dupont]]"), "marie-dupont");
        assert_eq!(link_target("[[marie-dupont|Marie]]"), "marie-dupont");
        assert_eq!(link_target("[[ ]]"), "");
    }
}
