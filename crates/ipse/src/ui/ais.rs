//! The connected AIs' screen: which AI reads what and proposes where, changed without
//! opening a file (`docs/spec/rights.md`, sections 1 and 2). Changes go through
//! [`Clients`] and [`Categories`], the same path as `ipse client`, and apply from each
//! AI's next request.

use ipse_core::rights::{Categories, Client, Clients, Gateway, last_access};
use ipse_core::vault::DateTime;
use ipse_core::{Error, Result};
use serde::{Deserialize, Serialize};

/// Longest name of an AI or of a category, in characters.
const MAX_NAME: usize = 80;

/// One connected AI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(
    test,
    derive(ts_rs::TS),
    ts(export, export_to = "../../../ui/src/lib/types/")
)]
pub(crate) struct AiView {
    id: String,
    name: String,
    /// The person said it runs entirely on this computer.
    local: bool,
    /// When it was connected, RFC 3339.
    created: String,
    /// Its last call, from the access log; `null` when none in the months kept.
    last_access: Option<String>,
    /// The categories it reads.
    read: Vec<String>,
    /// The categories it proposes in.
    propose: Vec<String>,
}

/// One category, and who may see it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(
    test,
    derive(ts_rs::TS),
    ts(export, export_to = "../../../ui/src/lib/types/")
)]
pub(crate) struct CategoryView {
    /// Its name in files: `people`, `etudes`.
    name: String,
    /// The name the person gave a category of their own; `null` for the built-in ones,
    /// which the interface names in its language.
    label: Option<String>,
    /// Closed to every AI until the person opens it to one.
    sensitive: bool,
    /// The person declared it.
    custom: bool,
    /// The person declared it without saying whether it is sensitive: it is.
    implicit: bool,
}

/// The screen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(
    test,
    derive(ts_rs::TS),
    ts(export, export_to = "../../../ui/src/lib/types/")
)]
pub(crate) struct AisView {
    ais: Vec<AiView>,
    categories: Vec<CategoryView>,
}

/// A newly connected AI, with what to paste in its app. Shown once: the access code is
/// not kept by Ipse.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(
    test,
    derive(ts_rs::TS),
    ts(export, export_to = "../../../ui/src/lib/types/")
)]
pub(crate) struct ConnectedView {
    ai: AiView,
    /// Its access code.
    code: String,
    /// The full path of this `ipse`, for the AI app's configuration.
    program: String,
    /// The full path of the memory folder.
    vault: String,
}

#[derive(Deserialize)]
struct ConnectBody {
    name: String,
    #[serde(default)]
    local: bool,
    /// For a local AI only: every category, sensitive ones included, as an explicit
    /// choice (`docs/spec/rights.md`, section 2).
    #[serde(default)]
    open_all: bool,
}

#[derive(Deserialize)]
struct RightsBody {
    read: Vec<String>,
    propose: Vec<String>,
}

#[derive(Deserialize)]
struct CategoryBody {
    label: String,
    sensitive: bool,
    /// The connected AIs that may read it, for one that is not sensitive.
    #[serde(default)]
    open_to: Vec<String>,
}

/// What `GET /api/ais` answers.
///
/// # Errors
///
/// Errors reading the connected AIs, the settings or the access log.
pub(crate) fn view(gateway: &mut Gateway) -> Result<AisView> {
    // Read through the gateway, so that a category declared since is known.
    gateway.fresh_owner()?;
    let root = gateway.vault().root();
    let clients = Clients::load(root)?;
    let ids: Vec<&str> = clients.list().iter().map(|c| c.id.as_str()).collect();
    let mut last = last_access(root, &ids)?;
    let ais = clients
        .list()
        .iter()
        .map(|c| ai_view(c, last.remove(&c.id)))
        .collect();
    let settings = gateway.settings();
    let categories = Categories::of(settings);
    let sensitive = categories.sensitive();
    let categories = categories
        .all()
        .into_iter()
        .map(|name| {
            let custom = settings.categories().get(name);
            CategoryView {
                name: name.to_owned(),
                label: custom.and_then(|c| c.label.clone()),
                sensitive: sensitive.contains(&name),
                custom: custom.is_some(),
                implicit: custom.is_some_and(|c| !c.said),
            }
        })
        .collect();
    Ok(AisView { ais, categories })
}

/// The person's changes: `POST /api/ais` connects an AI, `/api/ais/<id>/rights` sets
/// what it reads and proposes in, `/api/ais/<id>/remove` disconnects it,
/// `/api/categories` declares a category. `None` for an unknown address.
///
/// # Errors
///
/// [`Error::Refused`] with the reason in plain words: the body cannot be read, a name
/// is empty or too long, an AI or a category is unknown, an AI would propose in a
/// category it does not read; errors writing the files.
pub(crate) fn change(gateway: &mut Gateway, path: &str, body: &[u8]) -> Result<Option<Vec<u8>>> {
    let root = gateway.vault().root().to_owned();
    if path == "ais" {
        return connect(gateway, &read_body(body)?).map(|v| Some(to_json(&v)));
    }
    if path == "categories" {
        let body: CategoryBody = read_body(body)?;
        let label = name_of(&body.label, "category")?;
        let name = Categories::declare(&root, &label, body.sensitive)?;
        if !body.sensitive && !body.open_to.is_empty() {
            let categories = Categories::of(&ipse_core::vault::Settings::read(
                &root.join(ipse_core::vault::SETTINGS_FILE),
            )?);
            let mut clients = Clients::load(&root)?;
            for id in &body.open_to {
                let Some(client) = clients.list().iter().find(|c| &c.id == id) else {
                    continue;
                };
                let mut read = client.read.clone();
                read.push(name.clone());
                clients.set_rights(id, Some(read), None, &categories)?;
            }
        }
        return view(gateway).map(|v| Some(to_json(&v)));
    }
    let Some((id, action)) = path
        .strip_prefix("ais/")
        .and_then(|rest| rest.split_once('/'))
    else {
        return Ok(None);
    };
    match action {
        "rights" => {
            let body: RightsBody = read_body(body)?;
            let categories = Categories::of(gateway.settings());
            Clients::load(&root)?.set_rights(
                id,
                Some(body.read),
                Some(body.propose),
                &categories,
            )?;
        }
        "remove" => {
            if !Clients::load(&root)?.remove(id)? {
                return Err(Error::Refused(format!("There is no AI {id}.")));
            }
        }
        _ => return Ok(None),
    }
    view(gateway).map(|v| Some(to_json(&v)))
}

fn connect(gateway: &mut Gateway, body: &ConnectBody) -> Result<ConnectedView> {
    let name = name_of(&body.name, "AI")?;
    let root = gateway.vault().root().to_owned();
    let categories = Categories::of(gateway.settings());
    let mut clients = Clients::load(&root)?;
    let (mut client, token) = clients.add(
        &name,
        DateTime::now(gateway.settings().time_zone()),
        &categories,
        body.local,
    )?;
    if body.local && body.open_all {
        let all = categories.all().into_iter().map(str::to_owned).collect();
        client = clients.set_rights(&client.id, Some(all), None, &categories)?;
    }
    let program = std::env::current_exe()
        .ok()
        .and_then(|p| p.canonicalize().ok())
        .map_or_else(|| "ipse".to_owned(), |p| plain_path(&p));
    let vault = root
        .canonicalize()
        .map_or_else(|_| plain_path(&root), |p| plain_path(&p));
    Ok(ConnectedView {
        ai: ai_view(&client, None),
        code: token.expose().to_owned(),
        program,
        vault,
    })
}

/// A path as the person writes it: without Windows' `\\?\` prefix.
fn plain_path(path: &std::path::Path) -> String {
    let text = path.to_string_lossy();
    text.strip_prefix(r"\\?\").unwrap_or(&text).to_owned()
}

fn name_of(text: &str, what: &str) -> Result<String> {
    let name = text.trim();
    if name.is_empty() || name.chars().count() > MAX_NAME {
        return Err(Error::Refused(format!(
            "Give the {what} a name of 1 to {MAX_NAME} characters."
        )));
    }
    Ok(name.to_owned())
}

fn ai_view(client: &Client, last_access: Option<String>) -> AiView {
    AiView {
        id: client.id.clone(),
        name: client.name.clone(),
        local: client.local,
        created: client.created.clone(),
        last_access,
        read: client.read.clone(),
        propose: client.propose.clone(),
    }
}

fn read_body<T: for<'de> Deserialize<'de>>(body: &[u8]) -> Result<T> {
    serde_json::from_slice(body)
        .map_err(|_| Error::Refused("The request could not be read.".to_owned()))
}

fn to_json(value: &impl Serialize) -> Vec<u8> {
    serde_json::to_vec(value).unwrap_or_default()
}
