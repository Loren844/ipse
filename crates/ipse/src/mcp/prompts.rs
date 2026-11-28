//! Ready-made prompts (issue #274): what to ask once Ipse is connected, listed by the AI
//! app in its menu, in the memory's language.
//!
//! A prompt only asks: it is a message from the person that says which tools to call
//! and for what period, with today's date in the memory's time zone. The tools and the
//! rights stay as they are; a prompt reads nothing of the memory.

use ipse_core::vault::Language;
use jiff::civil::{Date, Weekday};
use rmcp::model::{GetPromptResult, Prompt, PromptArgument, PromptMessage, Role};
use serde_json::{Map, Value};

/// The argument of `what_do_you_know_about`.
const NAME: &str = "name";

/// The prompts, in the order the AI app shows them.
const PROMPTS: [&str; 4] = [
    "prepare_my_day",
    "prepare_my_week",
    "summarize_my_month",
    "what_do_you_know_about",
];

/// The prompts as the AI app lists them, in `language`.
pub(crate) fn list(language: Language) -> Vec<Prompt> {
    PROMPTS
        .iter()
        .map(|name| {
            let (title, description) = words(name, language);
            let arguments = (*name == "what_do_you_know_about").then(|| {
                let (title, description) = match language {
                    Language::Fr => (
                        "Qui ou quoi",
                        "Une personne, un lieu ou une chose, par son nom",
                    ),
                    _ => ("Who or what", "A person, a place or a thing, by its name"),
                };
                vec![
                    PromptArgument::new(NAME)
                        .with_title(title)
                        .with_description(description)
                        .with_required(true),
                ]
            });
            Prompt::new(*name, Some(description), arguments).with_title(title)
        })
        .collect()
}

/// The prompt `name` with its `arguments`, for `today`, in `language`; `Err` says what
/// is missing or unknown, for the AI app to show.
pub(crate) fn get(
    name: &str,
    arguments: Option<&Map<String, Value>>,
    language: Language,
    today: Date,
) -> Result<GetPromptResult, String> {
    let fr = language == Language::Fr;
    let text = match name {
        "prepare_my_day" => day(today, fr),
        "prepare_my_week" => week(today, fr),
        "summarize_my_month" => month(today, fr),
        "what_do_you_know_about" => {
            let who = arguments
                .and_then(|a| a.get(NAME))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|n| !n.is_empty())
                .ok_or_else(|| {
                    if fr {
                        "Dites de qui ou de quoi : l'argument « name ».".to_owned()
                    } else {
                        "Say who or what: the \"name\" argument.".to_owned()
                    }
                })?;
            about(who, fr)
        }
        _ => {
            return Err(format!(
                "No prompt is called \"{name}\". Ipse offers: {}.",
                PROMPTS.join(", ")
            ));
        }
    };
    let (_, description) = words(name, language);
    Ok(
        GetPromptResult::new(vec![PromptMessage::new_text(Role::User, text)])
            .with_description(description),
    )
}

/// The title and the description of the prompt `name`.
fn words(name: &str, language: Language) -> (&'static str, &'static str) {
    match (name, language) {
        ("prepare_my_day", Language::Fr) => (
            "Prépare ma journée",
            "Les rappels du jour, l'agenda et ce qu'il ne faut pas oublier.",
        ),
        ("prepare_my_day", _) => (
            "Prepare my day",
            "Today's reminders, agenda and what not to forget.",
        ),
        ("prepare_my_week", Language::Fr) => (
            "Prépare ma semaine",
            "Les sept jours qui viennent : rendez-vous, personnes, rappels.",
        ),
        ("prepare_my_week", _) => (
            "Prepare my week",
            "The next seven days: appointments, people, reminders.",
        ),
        ("summarize_my_month", Language::Fr) => (
            "Résume mon mois",
            "Ce mois-ci jusqu'à aujourd'hui : ce qui s'est passé, qui j'ai vu, ce que j'ai écouté et regardé.",
        ),
        ("summarize_my_month", _) => (
            "Summarize my month",
            "This month so far: what happened, who I saw, what I listened to and watched.",
        ),
        (_, Language::Fr) => (
            "Que sais-tu de…",
            "Tout ce que la mémoire dit d'une personne, d'un lieu ou d'une chose.",
        ),
        _ => (
            "What do you know about…",
            "Everything the memory says about a person, a place or a thing.",
        ),
    }
}

fn day(today: Date, fr: bool) -> String {
    if fr {
        format!(
            "Prépare ma journée, {when}. Avec Ipse : appelle `now` pour les rappels dus et ce \
qui se passe en ce moment, puis `events` du {today} au {today} pour mon agenda. Pour chaque \
rendez-vous, dis quand, où et avec qui ; pour une personne que je vais voir, regarde sa fiche \
avec `get` et rappelle-moi en une ligne ce qui est utile (un anniversaire proche, ce dont on a \
parlé). Termine par ce que je ne dois pas oublier. Sois bref ; s'il n'y a rien, dis-le.",
            when = long_fr(today),
        )
    } else {
        format!(
            "Prepare my day, {when}. With Ipse: call `now` for the reminders that are due and \
what is going on right now, then `events` from {today} to {today} for my agenda. For each \
appointment, say when, where and with whom; for someone I will see, look at their file with \
`get` and remind me in one line of what is useful (a birthday coming up, what we talked \
about). End with what I must not forget. Keep it short; if there is nothing, say so.",
            when = long_en(today),
        )
    }
}

fn week(today: Date, fr: bool) -> String {
    let last = today.saturating_add(jiff::Span::new().days(6));
    if fr {
        format!(
            "Prépare ma semaine, du {from} au {to}. Avec Ipse : appelle `now` pour les rappels, \
puis `events` du {today} au {last}. Donne jour par jour ce qui est prévu, avec qui et où ; \
signale les journées chargées, les anniversaires de la semaine et ce qu'il faut préparer à \
l'avance. Sois bref.",
            from = long_fr(today),
            to = long_fr(last),
        )
    } else {
        format!(
            "Prepare my week, from {from} to {to}. With Ipse: call `now` for the reminders, \
then `events` from {today} to {last}. Give day by day what is planned, with whom and where; \
point out the busy days, the birthdays of the week and what to get ready in advance. Keep it \
short.",
            from = long_en(today),
            to = long_en(last),
        )
    }
}

fn month(today: Date, fr: bool) -> String {
    let first = today.first_of_month();
    if fr {
        format!(
            "Résume mon mois de {month} {year}, jusqu'à aujourd'hui. Avec Ipse : `events` du \
{first} au {today}, puis `states_history` sur la même période pour la musique (`music`), ce \
que j'ai regardé (`watching`) et lu (`reading`). Raconte en quelques paragraphes ce qui s'est \
passé, qui j'ai vu le plus (d'après `seen_most`, en disant la règle), les lieux où je suis \
allé, ce que j'ai écouté et regardé. Ne dis que ce que la mémoire montre.",
            month = MONTHS_FR[usize::from(today.month().unsigned_abs()) - 1],
            year = today.year(),
        )
    } else {
        format!(
            "Summarize my month of {month} {year}, up to today. With Ipse: `events` from \
{first} to {today}, then `states_history` over the same period for music (`music`), what I \
watched (`watching`) and read (`reading`). Tell in a few paragraphs what happened, who I saw \
most (from `seen_most`, saying the rule), the places I went to, what I listened to and \
watched. Only say what the memory shows.",
            month = MONTHS_EN[usize::from(today.month().unsigned_abs()) - 1],
            year = today.year(),
        )
    }
}

fn about(who: &str, fr: bool) -> String {
    if fr {
        format!(
            "Que sais-tu de « {who} » ? Avec Ipse : `search` ce nom, puis `get` la fiche qui \
correspond (s'il y en a plusieurs, demande-moi laquelle). Dis qui ou ce que c'est, ses liens \
avec moi et avec d'autres, les faits notés avec leur date, puis, avec `events` et son id dans \
`with`, la dernière fois et la prochaine fois qu'on se voit. Dis d'où vient ce que tu dis, et \
ce que la mémoire ne sait pas."
        )
    } else {
        format!(
            "What do you know about \"{who}\"? With Ipse: `search` this name, then `get` the \
file that matches (if there are several, ask me which). Say who or what it is, how it relates \
to me and to others, the facts noted with their dates, then, with `events` and its id in \
`with`, the last and the next time we meet. Say where what you say comes from, and what the \
memory does not know."
        )
    }
}

const MONTHS_FR: [&str; 12] = [
    "janvier",
    "février",
    "mars",
    "avril",
    "mai",
    "juin",
    "juillet",
    "août",
    "septembre",
    "octobre",
    "novembre",
    "décembre",
];

const MONTHS_EN: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// `samedi 28 novembre 2026 (2026-11-28)`.
fn long_fr(date: Date) -> String {
    let weekday = match date.weekday() {
        Weekday::Monday => "lundi",
        Weekday::Tuesday => "mardi",
        Weekday::Wednesday => "mercredi",
        Weekday::Thursday => "jeudi",
        Weekday::Friday => "vendredi",
        Weekday::Saturday => "samedi",
        Weekday::Sunday => "dimanche",
    };
    let day = if date.day() == 1 {
        "1er".to_owned()
    } else {
        date.day().to_string()
    };
    format!(
        "{weekday} {day} {} {} ({date})",
        MONTHS_FR[usize::from(date.month().unsigned_abs()) - 1],
        date.year()
    )
}

/// `Saturday, November 28, 2026 (2026-11-28)`.
fn long_en(date: Date) -> String {
    format!("{} ({date})", date.strftime("%A, %B %-d, %Y"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(result: &GetPromptResult) -> String {
        result.messages[0].content.as_text().unwrap().text.clone()
    }

    #[test]
    fn the_prompts_are_in_the_memory_s_language() {
        let fr: Vec<String> = list(Language::Fr)
            .into_iter()
            .map(|p| p.title.unwrap())
            .collect();
        assert_eq!(
            fr,
            [
                "Prépare ma journée",
                "Prépare ma semaine",
                "Résume mon mois",
                "Que sais-tu de…"
            ]
        );
        let en = list(Language::En);
        assert_eq!(en[0].name, "prepare_my_day");
        assert_eq!(en[0].title.as_deref(), Some("Prepare my day"));
        let argument = &en[3].arguments.as_ref().unwrap()[0];
        assert_eq!(
            (argument.name.as_str(), argument.required),
            ("name", Some(true))
        );
    }

    #[test]
    fn a_prompt_gives_the_period_from_today() {
        let today = jiff::civil::date(2026, 11, 28);
        let day = text(&get("prepare_my_day", None, Language::Fr, today).unwrap());
        assert!(day.contains("samedi 28 novembre 2026"), "{day}");
        assert!(
            day.contains("`events` du 2026-11-28 au 2026-11-28"),
            "{day}"
        );
        let week = text(&get("prepare_my_week", None, Language::En, today).unwrap());
        assert!(week.contains("from 2026-11-28 to 2026-12-04"), "{week}");
        assert!(week.contains("Saturday, November 28, 2026"), "{week}");
        let month = text(&get("summarize_my_month", None, Language::Fr, today).unwrap());
        assert!(month.contains("novembre 2026"), "{month}");
        assert!(month.contains("du 2026-11-01 au 2026-11-28"), "{month}");
        assert_eq!(
            long_fr(jiff::civil::date(2026, 12, 1)),
            "mardi 1er décembre 2026 (2026-12-01)"
        );
    }

    #[test]
    fn what_do_you_know_needs_a_name() {
        let today = jiff::civil::date(2026, 11, 28);
        let mut arguments = Map::new();
        assert!(
            get(
                "what_do_you_know_about",
                Some(&arguments),
                Language::En,
                today
            )
            .is_err()
        );
        arguments.insert(NAME.to_owned(), Value::from(" Marie Dupont "));
        let about = text(
            &get(
                "what_do_you_know_about",
                Some(&arguments),
                Language::En,
                today,
            )
            .unwrap(),
        );
        assert!(
            about.starts_with("What do you know about \"Marie Dupont\"?"),
            "{about}"
        );
        assert!(get("unknown", None, Language::En, today).is_err());
    }
}
