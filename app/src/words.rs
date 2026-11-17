//! The app's own words, in the system's language: French, or English otherwise. The
//! interface of the memory has its own (`ui/src/lib/i18n.svelte.ts`).

/// The words of one language.
#[derive(Debug, Clone, Copy)]
// The menu's words are used on macOS only, where apps have a menu bar.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) struct Words {
    /// `fr` or `en`: the first screen speaks it too.
    pub(crate) code: &'static str,
    /// The `ipse` command is not beside the app.
    pub(crate) missing: &'static str,
    /// The memory could not be opened; `ipse`'s own words follow.
    pub(crate) not_opened: &'static str,
    /// The memory could not be created; `ipse`'s own words follow.
    pub(crate) not_created: &'static str,
    /// The folder chosen to open holds no memory.
    pub(crate) no_memory_here: &'static str,
    /// The folder chosen to create a memory already holds one.
    pub(crate) memory_there: &'static str,
    /// The folder chosen to create a memory holds other files.
    pub(crate) not_empty: &'static str,
    /// The folder last used is gone.
    pub(crate) gone: &'static str,
    /// Menu: open or create another memory.
    pub(crate) other_memory: &'static str,
    /// Menu: the File menu.
    pub(crate) file: &'static str,
    /// Menu: the Edit menu.
    pub(crate) edit: &'static str,
    /// Menu: the View menu.
    pub(crate) view: &'static str,
    /// Menu: reload the page.
    pub(crate) reload: &'static str,
    /// Menu: the Window menu.
    pub(crate) window: &'static str,
}

const FRENCH: Words = Words {
    code: "fr",
    missing: "La commande ipse manque à côté de l’application. Réinstallez Ipse.",
    not_opened: "Ipse n’a pas pu ouvrir cette mémoire.",
    not_created: "Ipse n’a pas pu créer la mémoire.",
    no_memory_here: "Ce dossier ne contient pas de mémoire Ipse. Choisissez le dossier de la mémoire, ou créez-en une.",
    memory_there: "Ce dossier contient déjà une mémoire : ouvrez-la plutôt.",
    not_empty: "Ce dossier contient déjà d’autres fichiers. Choisissez un dossier vide, ou un nouveau nom.",
    gone: "Le dossier de votre mémoire n’est plus là où Ipse l’a laissé. Ouvrez-le là où il est maintenant.",
    other_memory: "Ouvrir ou créer une autre mémoire…",
    file: "Fichier",
    edit: "Édition",
    view: "Présentation",
    reload: "Recharger",
    window: "Fenêtre",
};

const ENGLISH: Words = Words {
    code: "en",
    missing: "The ipse command is missing beside the app. Install Ipse again.",
    not_opened: "Ipse could not open this memory.",
    not_created: "Ipse could not create the memory.",
    no_memory_here: "This folder holds no Ipse memory. Pick the memory’s folder, or create one.",
    memory_there: "This folder already holds a memory: open it instead.",
    not_empty: "This folder already holds other files. Pick an empty folder, or a new name.",
    gone: "Your memory’s folder is no longer where Ipse left it. Open it where it is now.",
    other_memory: "Open or create another memory…",
    file: "File",
    edit: "Edit",
    view: "View",
    reload: "Reload",
    window: "Window",
};

impl Words {
    /// The words of the system's language.
    pub(crate) fn here() -> Words {
        Words::of(sys_locale::get_locale().as_deref())
    }

    /// The words for a locale like `fr-FR`.
    pub(crate) fn of(locale: Option<&str>) -> Words {
        match locale {
            Some(locale) if locale.to_ascii_lowercase().starts_with("fr") => FRENCH,
            _ => ENGLISH,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn french_for_french_systems_english_otherwise() {
        assert_eq!(Words::of(Some("fr-FR")).code, "fr");
        assert_eq!(Words::of(Some("fr")).code, "fr");
        assert_eq!(Words::of(Some("en-GB")).code, "en");
        assert_eq!(Words::of(Some("de-DE")).code, "en");
        assert_eq!(Words::of(None).code, "en");
    }
}
