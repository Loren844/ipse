//! `ipse init --demo`: a complete invented memory, to try Ipse without importing one's
//! own life, to film it and to test the interface.
//!
//! Everything goes through the normal paths: an address book and a calendar are
//! generated as files and imported like `ipse import`; an AI named Claude is connected
//! and its proposals are accepted, refused or left waiting like the person would. Names
//! come from fixed lists, picked with a fixed seed, so two demo memories made on the same
//! day are the same; dates follow today. Every email is at `example.org`; phone numbers
//! are in ranges reserved for fiction (06 39 98 in France, 555-01xx in North America).

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use ipse_core::proposals::{Author, Change, Proposals};
use ipse_core::rights::{Categories, Clients};
use ipse_core::search::SearchIndex;
use ipse_core::slug::to_file_stem;
use ipse_core::tasks::{TaskKind, TaskStatus, Tasks};
use ipse_core::vault::{DateTime, Language, MemoryFile, Vault};
use ipse_core::{Error, Result};
use jiff::ToSpan;
use jiff::civil::{Date, Weekday};

/// The words of the demo, in the vault's language.
struct Words {
    owner: (&'static str, &'static str),
    first: &'static [&'static str],
    last: &'static [&'static str],
    mother: &'static str,
    sister: (&'static str, &'static str),
    brother: (&'static str, &'static str),
    grandmother: &'static str,
    nicknames: [&'static str; 3],
    friend_notes: &'static [&'static str],
    work: &'static str,
    titles: &'static [&'static str],
    services: &'static [&'static str],
    yoga: (&'static str, &'static str),
    team: &'static str,
    lunch: &'static str,
    dinner: &'static str,
    at_home: &'static str,
    family: &'static str,
    dentist: (&'static str, &'static str),
    weekend: &'static str,
    holidays: &'static str,
    dinner_notes: &'static [&'static str],
    leak: (&'static str, &'static str),
    day_summary: fn(&[String]) -> String,
    summary_reason: &'static str,
    facts: [(&'static str, &'static str); 3],
    refused: &'static str,
    claude_reason: fn(&str) -> String,
    phone: fn(usize) -> Option<String>,
}

const FR: Words = Words {
    owner: ("Camille", "Laurent"),
    first: &[
        "Léa",
        "Hugo",
        "Chloé",
        "Louis",
        "Manon",
        "Gabriel",
        "Inès",
        "Arthur",
        "Jade",
        "Jules",
        "Louise",
        "Adam",
        "Alice",
        "Raphaël",
        "Emma",
        "Paul",
        "Zoé",
        "Nathan",
        "Lina",
        "Tom",
        "Sarah",
        "Théo",
        "Juliette",
        "Noé",
        "Clara",
        "Maxime",
        "Anaïs",
        "Lucas",
        "Margaux",
        "Victor",
        "Romane",
        "Antoine",
        "Eva",
        "Baptiste",
        "Lucie",
        "Mathis",
        "Agathe",
        "Simon",
        "Elsa",
        "Martin",
        "Rose",
        "Bastien",
        "Léna",
        "Quentin",
        "Mila",
        "Étienne",
        "Nina",
        "Félix",
        "Apolline",
        "Samuel",
        "Charlotte",
        "Yanis",
        "Capucine",
        "Robin",
        "Héloïse",
        "Malo",
        "Justine",
        "Oscar",
        "Pauline",
        "Basile",
    ],
    last: &[
        "Martin",
        "Bernard",
        "Dubois",
        "Thomas",
        "Robert",
        "Richard",
        "Petit",
        "Durand",
        "Leroy",
        "Moreau",
        "Simon",
        "Michel",
        "Lefebvre",
        "Garcia",
        "David",
        "Bertrand",
        "Roux",
        "Vincent",
        "Fournier",
        "Morel",
        "Girard",
        "André",
        "Mercier",
        "Dupont",
        "Lambert",
        "Bonnet",
        "François",
        "Legrand",
        "Garnier",
        "Faure",
        "Rousseau",
        "Blanc",
        "Guérin",
        "Muller",
        "Henry",
        "Roussel",
        "Perrin",
        "Morin",
        "Mathieu",
        "Clément",
    ],
    mother: "Maman",
    sister: ("Élise", "Laurent"),
    brother: ("Hugo", "Laurent"),
    grandmother: "Mamie",
    nicknames: ["Jo plombier", "Léo escalade", "Sandrine compta"],
    friend_notes: &[
        "Rencontrée au club d'escalade en 2019.",
        "Travaille comme infirmière à Nantes.",
        "Fan de jazz, joue du saxophone le week-end.",
        "Deux enfants, Iris et Léon.",
        "Végétarienne, adore la cuisine libanaise.",
        "Habite à Lyon depuis 2022.",
        "Prof de maths au collège Jean-Moulin.",
        "Court le semi-marathon de Paris chaque année.",
        "Allergique aux noix.",
        "Parrain de Léon.",
        "Revient d'un an au Québec.",
        "Fait de la poterie le jeudi soir.",
    ],
    work: "Atelier Nord",
    titles: &[
        "Designer",
        "Cheffe de projet",
        "Développeur",
        "Développeuse",
        "Directrice",
        "Comptable",
        "Stagiaire",
        "Responsable produit",
    ],
    services: &[
        "Cabinet dentaire du Parc",
        "Garage des Lilas",
        "Boulangerie Lemoine",
        "Pharmacie Centrale",
        "Mairie du 11e",
        "Club d'escalade Vertige",
        "Librairie Le Phare",
        "Cabinet Morel, kinésithérapie",
    ],
    yoga: ("Yoga", "Studio Lotus"),
    team: "Point d'équipe",
    lunch: "Déjeuner avec",
    dinner: "Dîner chez",
    at_home: "Chez",
    family: "Déjeuner chez Maman",
    dentist: ("Dentiste", "Cabinet dentaire du Parc"),
    weekend: "Week-end à Lyon",
    holidays: "Vacances à Biarritz",
    dinner_notes: &[
        "Apporter le dessert.",
        "Elle fête sa nouvelle maison.",
        "Penser au vin.",
        "Soirée jeux de société.",
    ],
    leak: ("Fuite sous l'évier", "Jo passe vers 9 h."),
    day_summary: |titles| match titles {
        [] => String::new(),
        [one] => format!("Une journée tranquille : {}.", lower(one)),
        [first, rest @ ..] => format!(
            "{first}, puis {}. Une journée bien remplie.",
            rest.iter()
                .map(|t| lower(t))
                .collect::<Vec<_>>()
                .join(", puis ")
        ),
    },
    summary_reason: "Le résumé de la journée, d'après votre agenda.",
    facts: [
        (
            "Commence un nouveau travail à la médiathèque en janvier.",
            "me l'a dit pendant notre conversation.",
        ),
        ("Parle japonais.", "l'a mentionné dans un message."),
        ("Prépare un marathon au printemps.", "en a parlé hier."),
    ],
    refused: "Coréen, pas japonais.",
    claude_reason: |first| format!("{first} me l'a dit pendant notre conversation."),
    phone: |i| (i < 10_000).then(|| format!("+33 6 39 98 {:02} {:02}", i / 100, i % 100)),
};

const EN: Words = Words {
    owner: ("Alex", "Morgan"),
    first: &[
        "Olivia", "Liam", "Emma", "Noah", "Ava", "Oliver", "Sophia", "Elijah", "Isabella", "James",
        "Mia", "William", "Amelia", "Benjamin", "Harper", "Lucas", "Evelyn", "Henry", "Abigail",
        "Theodore", "Emily", "Jack", "Ella", "Levi", "Grace", "Daniel", "Chloe", "Owen", "Nora",
        "Samuel", "Lily", "Ryan", "Hannah", "Nathan", "Zoe", "Caleb", "Leah", "Isaac", "Stella",
        "Julian", "Lucy", "Aaron", "Ruby", "Miles", "Alice", "Ethan", "Clara", "Adam", "Iris",
        "Simon", "Rose", "Felix", "Maya", "Oscar", "Ivy", "Hugo", "June", "Arthur", "Eva", "Max",
    ],
    last: &[
        "Smith", "Johnson", "Williams", "Brown", "Jones", "Miller", "Davis", "Wilson", "Anderson",
        "Taylor", "Thomas", "Moore", "Jackson", "Martin", "Lee", "Thompson", "White", "Harris",
        "Clark", "Lewis", "Robinson", "Walker", "Young", "Allen", "King", "Wright", "Scott",
        "Hill", "Green", "Adams", "Baker", "Nelson", "Carter", "Mitchell", "Turner", "Parker",
        "Collins", "Edwards", "Stewart", "Morris",
    ],
    mother: "Mom",
    sister: ("Ellie", "Morgan"),
    brother: ("Sam", "Morgan"),
    grandmother: "Granny",
    nicknames: ["Joe plumber", "Leo climbing", "Sandra accounts"],
    friend_notes: &[
        "Met at the climbing gym in 2019.",
        "Works as a nurse in Boston.",
        "Jazz fan, plays the saxophone on weekends.",
        "Two kids, Iris and Leon.",
        "Vegetarian, loves Lebanese food.",
        "Moved to Chicago in 2022.",
        "Teaches maths at Lincoln Middle School.",
        "Runs the city half marathon every year.",
        "Allergic to nuts.",
        "Leon's godfather.",
        "Back from a year in Montreal.",
        "Does pottery on Thursday evenings.",
    ],
    work: "North Studio",
    titles: &[
        "Designer",
        "Project lead",
        "Developer",
        "Developer",
        "Director",
        "Accountant",
        "Intern",
        "Product manager",
    ],
    services: &[
        "Park Dental",
        "Lilac Garage",
        "Corner Bakery",
        "Central Pharmacy",
        "City Hall",
        "Vertigo Climbing",
        "Lighthouse Books",
        "Morris Physiotherapy",
    ],
    yoga: ("Yoga", "Lotus Studio"),
    team: "Team meeting",
    lunch: "Lunch with",
    dinner: "Dinner at",
    at_home: "At",
    family: "Lunch at Mom's",
    dentist: ("Dentist", "Park Dental"),
    weekend: "Weekend in Chicago",
    holidays: "Holidays in Maine",
    dinner_notes: &[
        "Bring dessert.",
        "Housewarming.",
        "Remember the wine.",
        "Board game night.",
    ],
    leak: ("Leak under the sink", "Joe comes around 9."),
    day_summary: |titles| match titles {
        [] => String::new(),
        [one] => format!("A quiet day: {}.", lower(one)),
        [first, rest @ ..] => format!(
            "{first}, then {}. A full day.",
            rest.iter()
                .map(|t| lower(t))
                .collect::<Vec<_>>()
                .join(", then ")
        ),
    },
    summary_reason: "The day's summary, from your calendar.",
    facts: [
        (
            "Starts a new job at the public library in January.",
            "told me during our conversation.",
        ),
        ("Speaks Japanese.", "mentioned it in a message."),
        (
            "Training for a marathon in the spring.",
            "talked about it yesterday.",
        ),
    ],
    refused: "Korean, not Japanese.",
    claude_reason: |first| format!("{first} told me during our conversation."),
    phone: |i| (i < 100).then(|| format!("+1 202 555 01{i:02}")),
};

/// A title as it reads inside a sentence.
fn lower(text: &str) -> String {
    let mut chars = text.chars();
    chars
        .next()
        .map_or_else(String::new, |c| c.to_lowercase().chain(chars).collect())
}

/// A small deterministic random generator (xorshift): the same demo every time.
struct Seed(u64);

impl Seed {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: usize) -> usize {
        usize::try_from(self.next() % (n as u64)).unwrap_or(0)
    }
}

/// One invented person of the address book.
struct Card {
    name: String,
    first: String,
    email: Option<String>,
    phone: Option<String>,
    birthday: Option<String>,
    org: Option<String>,
    title: Option<String>,
    note: Option<String>,
}

impl Card {
    fn new(name: impl Into<String>, first: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            first: first.into(),
            email: None,
            phone: None,
            birthday: None,
            org: None,
            title: None,
            note: None,
        }
    }
}

/// Text in a vCard or iCalendar value.
fn escaped(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace(',', "\\,")
        .replace(';', "\\;")
        .replace('\n', "\\n")
}

fn email_of(first: &str, last: &str) -> String {
    let stem = to_file_stem(&format!("{first} {last}")).unwrap_or_default();
    format!("{}@example.org", stem.replace('-', "."))
}

/// The people of the demo: family, friends, colleagues, services, a few nicknames and
/// two people saved twice.
struct People {
    owner: Card,
    family: Vec<Card>,
    friends: Vec<Card>,
    colleagues: Vec<Card>,
    others: Vec<Card>,
}

impl People {
    fn all(&self) -> impl Iterator<Item = &Card> {
        self.family
            .iter()
            .chain(&self.friends)
            .chain(&self.colleagues)
            .chain(&self.others)
    }
}

#[allow(clippy::too_many_lines)] // one group of people after the other
fn people(words: &Words, seed: &mut Seed) -> People {
    let mut phones = 0;
    let mut phone = || {
        phones += 1;
        (words.phone)(phones)
    };
    let mut used = std::collections::HashSet::new();
    let mut person = |seed: &mut Seed| loop {
        let first = words.first[seed.below(words.first.len())];
        let last = words.last[seed.below(words.last.len())];
        if used.insert((first, last)) && (first, last) != words.owner {
            return (first, last);
        }
    };
    let (first, last) = words.owner;
    let mut owner = Card::new(format!("{first} {last}"), first);
    owner.email = Some(email_of(first, last));
    owner.phone = phone();

    let mut family = Vec::new();
    let mut mother = Card::new(words.mother, words.mother);
    mother.email = Some(format!("{}@example.org", lower(words.mother)));
    mother.phone = phone();
    mother.birthday = Some("1961-03-08".to_owned());
    family.push(mother);
    for (first, last) in [words.sister, words.brother] {
        let mut card = Card::new(format!("{first} {last}"), first);
        card.email = Some(email_of(first, last));
        card.phone = phone();
        card.birthday = Some(format!(
            "19{}-0{}-1{}",
            88 + seed.below(8),
            1 + seed.below(9),
            seed.below(10)
        ));
        family.push(card);
    }
    let mut grandmother = Card::new(words.grandmother, words.grandmother);
    grandmother.phone = phone();
    family.push(grandmother);

    let mut friends = Vec::new();
    for note in words.friend_notes {
        let (first, last) = person(seed);
        let mut card = Card::new(format!("{first} {last}"), first);
        card.email = Some(email_of(first, last));
        card.phone = phone();
        card.birthday = Some(format!(
            "19{}-{:02}-{:02}",
            85 + seed.below(12),
            1 + seed.below(12),
            1 + seed.below(28)
        ));
        card.note = Some((*note).to_owned());
        friends.push(card);
    }

    let mut colleagues = Vec::new();
    for i in 0..30 {
        let (first, last) = person(seed);
        let mut card = Card::new(format!("{first} {last}"), first);
        card.email = Some(email_of(first, last).replace("@example.org", "@nord.example.org"));
        card.org = Some(words.work.to_owned());
        card.title = Some(words.titles[i % words.titles.len()].to_owned());
        if i % 3 == 0 {
            card.phone = phone();
        }
        colleagues.push(card);
    }

    let mut others = Vec::new();
    for i in 0..96 {
        let (first, last) = person(seed);
        let mut card = Card::new(format!("{first} {last}"), first);
        card.phone = phone();
        if i % 2 == 0 {
            card.email = Some(email_of(first, last));
        }
        if i % 4 == 0 {
            card.org = Some(words.services[seed.below(words.services.len())].to_owned());
        }
        others.push(card);
    }
    for nickname in words.nicknames {
        let mut card = Card::new(nickname, nickname);
        card.phone = phone();
        others.push(card);
    }
    // Saved twice: once with the full name, once shortened, same email or phone.
    let friend = &friends[0];
    let initial = friend.name[friend.first.len()..]
        .trim()
        .chars()
        .next()
        .unwrap_or('X');
    let mut twice = Card::new(format!("{} {initial}.", friend.first), friend.first.clone());
    twice.email.clone_from(&friend.email);
    others.push(twice);
    let colleague = &colleagues[0];
    let mut twice = Card::new(
        format!("{} ({})", colleague.name, words.work),
        colleague.first.clone(),
    );
    twice.phone = colleague.phone.clone().or_else(&mut phone);
    if colleague.phone.is_none() {
        colleagues[0].phone.clone_from(&twice.phone);
    }
    others.push(twice);

    People {
        owner,
        family,
        friends,
        colleagues,
        others,
    }
}

/// The address book, as a vCard 3 file; the owner's card lists their family.
fn address_book(people: &People) -> String {
    let mut out = String::new();
    let mut card = |c: &Card, related: &[(&str, &str)]| {
        out.push_str("BEGIN:VCARD\r\nVERSION:3.0\r\n");
        let _ = write!(out, "FN:{}\r\n", escaped(&c.name));
        if let Some(rest) = c.name.strip_prefix(&c.first).map(str::trim)
            && !rest.is_empty()
        {
            let _ = write!(out, "N:{};{};;;\r\n", escaped(rest), escaped(&c.first));
        }
        if let Some(email) = &c.email {
            let _ = write!(out, "EMAIL;TYPE=INTERNET:{email}\r\n");
        }
        if let Some(phone) = &c.phone {
            let _ = write!(out, "TEL;TYPE=CELL:{phone}\r\n");
        }
        if let Some(birthday) = &c.birthday {
            let _ = write!(out, "BDAY:{birthday}\r\n");
        }
        if let Some(org) = &c.org {
            let _ = write!(out, "ORG:{}\r\n", escaped(org));
        }
        if let Some(title) = &c.title {
            let _ = write!(out, "TITLE:{}\r\n", escaped(title));
        }
        if let Some(note) = &c.note {
            let _ = write!(out, "NOTE:{}\r\n", escaped(note));
        }
        for (i, (name, label)) in related.iter().enumerate() {
            let _ = write!(
                out,
                "item{n}.X-ABRELATEDNAMES:{}\r\nitem{n}.X-ABLabel:{label}\r\n",
                escaped(name),
                n = i + 1
            );
        }
        out.push_str("END:VCARD\r\n");
    };
    let family = &people.family;
    card(
        &people.owner,
        &[
            (&family[0].name, "_$!<Mother>!$_"),
            (&family[1].name, "_$!<Sister>!$_"),
            (&family[2].name, "_$!<Brother>!$_"),
        ],
    );
    for c in people.all() {
        card(c, &[]);
    }
    out
}

/// Two years of the owner's calendar, ending two weeks after today.
#[allow(clippy::too_many_lines)] // one block per kind of event
fn calendar(words: &Words, people: &People, today: Date, seed: &mut Seed) -> String {
    let mut out = String::from("BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Ipse//Demo//EN\r\n");
    let owner = people.owner.email.as_deref().unwrap_or_default();
    let mut uid = 0;
    let mut event = |title: &str,
                     start: String,
                     end: String,
                     place: Option<&str>,
                     notes: Option<&str>,
                     organizer: Option<&Card>,
                     guests: &[&Card]| {
        uid += 1;
        let _ = write!(
            out,
            "BEGIN:VEVENT\r\nUID:demo-{uid}@example.org\r\nDTSTAMP:20260101T000000Z\r\n{start}\r\n{end}\r\nSUMMARY:{}\r\n",
            escaped(title)
        );
        if let Some(place) = place {
            let _ = write!(out, "LOCATION:{}\r\n", escaped(place));
        }
        if let Some(notes) = notes {
            let _ = write!(out, "DESCRIPTION:{}\r\n", escaped(notes));
        }
        match organizer {
            Some(o) => {
                let _ = write!(
                    out,
                    "ORGANIZER;CN={}:mailto:{}\r\n",
                    escaped(&o.name),
                    o.email.as_deref().unwrap_or_default()
                );
            }
            None => {
                let _ = write!(out, "ORGANIZER:mailto:{owner}\r\n");
            }
        }
        for g in guests {
            if let Some(email) = &g.email {
                let _ = write!(out, "ATTENDEE;CN={}:mailto:{email}\r\n", escaped(&g.name));
            }
        }
        out.push_str("END:VEVENT\r\n");
    };
    let at = |day: Date, hour: i8, minute: i8| {
        format!("{}T{hour:02}{minute:02}00", day.strftime("%Y%m%d"))
    };
    let first = today.saturating_sub(730.days());
    let last = today.saturating_add(14.days());
    let friends: Vec<&Card> = people.friends.iter().collect();
    let colleagues: Vec<&Card> = people.colleagues.iter().take(5).collect();
    let mut day = first;
    let mut week = 0usize;
    while day <= last {
        let summer = day.month() == 8;
        match day.weekday() {
            Weekday::Monday if !summer => {
                event(
                    words.team,
                    format!("DTSTART:{}", at(day, 9, 30)),
                    format!("DTEND:{}", at(day, 10, 0)),
                    None,
                    None,
                    Some(colleagues[0]),
                    &colleagues[1..],
                );
            }
            Weekday::Tuesday if !summer => {
                event(
                    words.yoga.0,
                    format!("DTSTART:{}", at(day, 19, 0)),
                    format!("DTEND:{}", at(day, 20, 0)),
                    Some(words.yoga.1),
                    None,
                    None,
                    &[],
                );
            }
            Weekday::Thursday if week.is_multiple_of(2) => {
                let friend = friends[week / 2 % friends.len()];
                event(
                    &format!("{} {}", words.lunch, friend.first),
                    format!("DTSTART:{}", at(day, 12, 30)),
                    format!("DTEND:{}", at(day, 13, 30)),
                    None,
                    None,
                    None,
                    &[friend],
                );
            }
            Weekday::Saturday if week % 4 == 1 => {
                let host = friends[seed.below(friends.len())];
                let guest = friends[seed.below(friends.len())];
                let notes = words.dinner_notes[seed.below(words.dinner_notes.len())];
                event(
                    &format!("{} {}", words.dinner, host.first),
                    format!("DTSTART:{}", at(day, 20, 0)),
                    format!("DTEND:{}", at(day, 23, 0)),
                    Some(&format!("{} {}", words.at_home, host.first)),
                    Some(notes),
                    Some(host),
                    &[guest],
                );
            }
            Weekday::Sunday if week % 6 == 3 => {
                event(
                    words.family,
                    format!("DTSTART:{}", at(day, 12, 30)),
                    format!("DTEND:{}", at(day, 16, 0)),
                    None,
                    None,
                    None,
                    &[&people.family[0], &people.family[1], &people.family[2]],
                );
            }
            Weekday::Wednesday if week % 26 == 10 => {
                event(
                    words.dentist.0,
                    format!("DTSTART:{}", at(day, 17, 30)),
                    format!("DTEND:{}", at(day, 18, 0)),
                    Some(words.dentist.1),
                    None,
                    None,
                    &[],
                );
            }
            Weekday::Friday if week % 17 == 5 => {
                let next = day.saturating_add(1.day()).saturating_add(2.days());
                event(
                    words.weekend,
                    format!(
                        "DTSTART;VALUE=DATE:{}",
                        day.saturating_add(1.day()).strftime("%Y%m%d")
                    ),
                    format!("DTEND;VALUE=DATE:{}", next.strftime("%Y%m%d")),
                    None,
                    None,
                    None,
                    &[friends[1]],
                );
            }
            _ => {}
        }
        if day.month() == 8 && day.day() == 3 {
            event(
                words.holidays,
                format!("DTSTART;VALUE=DATE:{}", day.strftime("%Y%m%d")),
                format!(
                    "DTEND;VALUE=DATE:{}",
                    day.saturating_add(14.days()).strftime("%Y%m%d")
                ),
                None,
                None,
                None,
                &[],
            );
        }
        if day.weekday() == Weekday::Sunday {
            week += 1;
        }
        day = day.saturating_add(1.day());
    }
    let leak = today.saturating_sub(40.days());
    event(
        words.leak.0,
        format!("DTSTART:{}", at(leak, 9, 0)),
        format!("DTEND:{}", at(leak, 10, 0)),
        None,
        Some(words.leak.1),
        None,
        &[],
    );
    out.push_str("END:VCALENDAR\r\n");
    out
}

/// What the demo made, for the person.
pub(crate) struct Made {
    pub(crate) owner: String,
    pub(crate) people: usize,
    pub(crate) events: usize,
}

/// The demo owner's name in `language`.
pub(crate) fn owner_name(language: Language) -> String {
    let (first, last) = if language == Language::En {
        EN.owner
    } else {
        FR.owner
    };
    format!("{first} {last}")
}

/// Fills the new memory in `vault` with the demo.
///
/// # Errors
///
/// Errors writing the generated files, importing them, or writing proposals.
pub(crate) fn fill(vault: &Vault) -> Result<Made> {
    let settings = vault.settings();
    let words = if settings.language() == Language::En {
        &EN
    } else {
        &FR
    };
    let today = jiff::Timestamp::now()
        .to_zoned(settings.time_zone().clone())
        .date();
    let mut seed = Seed(0x1b5e_2026_0d3e_0001);
    let people = people(words, &mut seed);

    // The owner's email, so that their card and their own events are recognized.
    let owner_path = vault.owner_file().to_owned();
    let mut owner = MemoryFile::read(&owner_path)?;
    if let Some(entity) = owner.entity_mut() {
        entity.emails = people.owner.email.iter().cloned().collect();
    }
    owner.write()?;
    vault.history()?.record_outside_edits()?;

    let dir = scratch()?;
    let result = (|| {
        let contacts = dir.join("contacts.vcf");
        write(&contacts, &address_book(&people))?;
        let agenda = dir.join("agenda.ics");
        write(&agenda, &calendar(words, &people, today, &mut seed))?;
        super::import::import_into(vault, &contacts)?;
        super::import::import_into(vault, &agenda)
    })();
    let _ = std::fs::remove_dir_all(&dir);
    result?;

    claude(vault, words, &people, today)?;
    let count = |folder: &str| walk(&vault.root().join(folder));
    Ok(Made {
        owner: people.owner.name.clone(),
        people: count("people").saturating_sub(1),
        events: count("events"),
    })
}

fn walk(dir: &Path) -> usize {
    std::fs::read_dir(dir).map_or(0, |entries| {
        entries
            .flatten()
            .map(|e| {
                let path = e.path();
                if path.is_dir() {
                    walk(&path)
                } else {
                    usize::from(path.extension().is_some_and(|x| x == "md"))
                }
            })
            .sum()
    })
}

/// A folder for the generated files, outside the memory.
fn scratch() -> Result<PathBuf> {
    let dir = std::env::temp_dir().join(format!("ipse-demo-{}", std::process::id()));
    std::fs::create_dir_all(&dir).map_err(|source| Error::Io {
        action: "create",
        path: dir.clone(),
        source,
    })?;
    Ok(dir)
}

fn write(path: &Path, text: &str) -> Result<()> {
    std::fs::write(path, text).map_err(|source| Error::Io {
        action: "write",
        path: path.to_owned(),
        source,
    })
}

/// An AI named Claude, connected with the right to propose; the summaries of the last
/// days it wrote, most of them accepted; facts it heard, one accepted, one refused, one
/// waiting.
fn claude(vault: &Vault, words: &Words, people: &People, today: Date) -> Result<()> {
    let categories = Categories::of(vault.settings());
    let mut clients = Clients::load(vault.root())?;
    let now = DateTime::now(vault.settings().time_zone());
    // Its code is not shown: the person connects their own AI when they want to.
    let (client, _) = clients.add("Claude", now, &categories, false)?;
    clients.set_rights(
        &client.id,
        None,
        Some(vec!["people".into(), "agenda".into(), "summaries".into()]),
        &categories,
    )?;
    let by = || Author::Client(client.id.clone());
    let proposals = Proposals::new(vault);

    // Summaries of the last days that had events, through their tasks.
    let index = SearchIndex::load(vault.root())?;
    let tasks = Tasks::new(vault);
    tasks.plan_summaries(&index, today)?;
    let mut days: Vec<(String, String)> = tasks
        .list()?
        .into_iter()
        .filter(|t| t.kind == TaskKind::Summary && t.status == TaskStatus::Open)
        .filter_map(|t| {
            let day = t.period.as_deref()?.strip_prefix("day:")?.to_owned();
            Some((day, t.id))
        })
        .collect();
    days.sort();
    let recent = days.len().saturating_sub(7);
    for (n, (day, task)) in days[recent..].iter().enumerate() {
        let Some(date) = day.parse::<Date>().ok() else {
            continue;
        };
        let titles: Vec<String> = index
            .events(|_| true, |_| true)
            .into_iter()
            .filter(|e| e.start.date() == date)
            .filter_map(|e| MemoryFile::read(vault.root().join(e.path)).ok())
            .filter_map(|f| f.event().map(|e| e.title.clone()))
            .collect();
        let text = (words.day_summary)(&titles);
        if text.is_empty() {
            continue;
        }
        let proposal = proposals.submit(
            by(),
            words.summary_reason,
            Some(task.clone()),
            vec![Change::WriteSummary {
                file: None,
                period: "day".into(),
                start: day.clone(),
                from: None,
                text,
            }],
        )?;
        // The last two wait for the person.
        if n + 2 < days.len() - recent {
            proposals.accept(&proposal.id)?;
        }
    }

    // Facts it heard in conversation.
    let file = |card: &Card| -> String {
        format!("people/{}.md", to_file_stem(&card.name).unwrap_or_default())
    };
    for (i, (fact, said)) in words.facts.iter().enumerate() {
        let friend = &people.friends[i + 2];
        let reason = if i == 0 {
            (words.claude_reason)(&friend.first)
        } else {
            format!("{} {said}", friend.first)
        };
        let proposal = proposals.submit(
            by(),
            &reason,
            None,
            vec![Change::AddFact {
                file: file(friend),
                text: (*fact).to_owned(),
            }],
        )?;
        match i {
            0 => {
                proposals.accept(&proposal.id)?;
            }
            1 => {
                proposals.refuse(&proposal.id, Some(words.refused))?;
            }
            _ => {}
        }
    }
    Ok(())
}
