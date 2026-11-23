//! Where the window may go. It shows two things only: the app's own first screen and
//! the memory's interface on `127.0.0.1`. A link to anything else opens in the
//! person's browser, so the window never loads another site. A few addresses are
//! answered by the app itself, on either origin: `/app/switch` (back to the first
//! screen), `/app/quit`, `/app/settings?notifications=on&start=off&updates=on` (the
//! app's own settings, from the interface's Settings), and `/app/update/install` and
//! `/app/update/later` (the person's answer to a new version).

use tauri::{AppHandle, Manager as _, Url};
use tauri_plugin_opener::OpenerExt as _;

use crate::State;

/// What to do with a navigation of the window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Go {
    /// Let the window go there.
    Allow,
    /// Stay where it is.
    Refuse,
    /// Back to the first screen, to open or create another memory.
    Switch,
    /// Quit the app.
    Quit,
    /// Change the app's own settings: notifications, starting with the computer.
    Settings {
        /// Show system notifications, when changed.
        notifications: Option<bool>,
        /// Start with the computer, when changed.
        start: Option<bool>,
        /// Check for updates, when changed.
        updates: Option<bool>,
    },
    /// Install the update found, then start the new version.
    Install,
    /// Not now: stop showing the update until the next check.
    Later,
    /// Open this address in the person's browser.
    Outside(Url),
}

/// Decides what a navigation to `url` does, `port` being the memory's server, if one
/// runs.
pub(crate) fn decide(url: &Url, port: Option<u16>) -> Go {
    let ours = is_the_app(url) || is_the_server(url, port);
    if ours {
        return match url.path() {
            "/app/switch" => Go::Switch,
            "/app/quit" => Go::Quit,
            "/app/settings" => Go::Settings {
                notifications: switch(url, "notifications"),
                start: switch(url, "start"),
                updates: switch(url, "updates"),
            },
            "/app/update/install" => Go::Install,
            "/app/update/later" => Go::Later,
            _ => Go::Allow,
        };
    }
    match url.scheme() {
        "about" | "data" | "blob" => Go::Allow,
        "http" | "https" | "mailto" => Go::Outside(url.clone()),
        // Another program's link (`obsidian:`, `file:`…): not from the window.
        _ => Go::Refuse,
    }
}

/// The switch `name` of the query: `on` or `off`; `None` when absent or neither.
fn switch(url: &Url, name: &str) -> Option<bool> {
    url.query_pairs()
        .find(|(key, _)| key == name)
        .and_then(|(_, value)| match value.as_ref() {
            "on" => Some(true),
            "off" => Some(false),
            _ => None,
        })
}

/// The app's own pages: `tauri://localhost`, or `http://tauri.localhost` on Windows.
fn is_the_app(url: &Url) -> bool {
    match url.scheme() {
        "tauri" => url.host_str() == Some("localhost"),
        "http" | "https" => url.host_str() == Some("tauri.localhost"),
        _ => false,
    }
}

/// The memory's interface: `http://127.0.0.1` on the server's port.
fn is_the_server(url: &Url, port: Option<u16>) -> bool {
    url.scheme() == "http"
        && url.host_str() == Some("127.0.0.1")
        && port.is_some()
        && url.port() == port
}

/// Does what [`decide`] said, outside the navigation itself.
pub(crate) fn act(app: &AppHandle, go: Go) {
    match go {
        Go::Allow | Go::Refuse => {}
        Go::Quit => app.exit(0),
        Go::Settings {
            notifications,
            start,
            updates,
        } => crate::prefs::change(app, notifications, start, updates),
        Go::Install => crate::updates::install(app),
        Go::Later => crate::updates::later(app),
        Go::Outside(url) => {
            let _ = app.opener().open_url(url.as_str(), None::<&str>);
        }
        Go::Switch => {
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                app.state::<State>().stop();
                if let (Some(window), Some(url)) =
                    (app.get_webview_window("main"), crate::welcome(true))
                {
                    let _ = window.navigate(url);
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(text: &str) -> Url {
        Url::parse(text).unwrap()
    }

    #[test]
    fn the_app_and_its_server_stay_in_the_window() {
        assert_eq!(
            decide(&url("tauri://localhost/index.html"), None),
            Go::Allow
        );
        assert_eq!(
            decide(&url("http://tauri.localhost/index.html"), None),
            Go::Allow
        );
        assert_eq!(
            decide(&url("http://127.0.0.1:4100/login?code=abc"), Some(4100)),
            Go::Allow
        );
        assert_eq!(decide(&url("about:blank"), None), Go::Allow);
    }

    #[test]
    fn another_port_or_site_goes_to_the_browser() {
        let other = url("http://127.0.0.1:9999/");
        assert_eq!(decide(&other, Some(4100)), Go::Outside(other.clone()));
        assert_eq!(decide(&other, None), Go::Outside(other));
        let site = url("https://contacts.google.com/");
        assert_eq!(decide(&site, Some(4100)), Go::Outside(site));
        assert_eq!(decide(&url("file:///etc/passwd"), Some(4100)), Go::Refuse);
    }

    #[test]
    fn the_app_answers_its_own_addresses() {
        assert_eq!(
            decide(&url("http://127.0.0.1:4100/app/switch"), Some(4100)),
            Go::Switch
        );
        assert_eq!(decide(&url("tauri://localhost/app/quit"), None), Go::Quit);
        assert_eq!(
            decide(
                &url("http://127.0.0.1:4100/app/settings?notifications=off&start=on&updates=off"),
                Some(4100)
            ),
            Go::Settings {
                notifications: Some(false),
                start: Some(true),
                updates: Some(false)
            }
        );
        assert_eq!(
            decide(&url("http://127.0.0.1:4100/app/update/install"), Some(4100)),
            Go::Install
        );
        assert_eq!(
            decide(&url("tauri://localhost/app/update/later"), None),
            Go::Later
        );
        let elsewhere = url("https://example.org/app/update/install");
        assert_eq!(decide(&elsewhere, Some(4100)), Go::Outside(elsewhere));
        assert_eq!(
            decide(
                &url("http://127.0.0.1:4100/app/settings?start=maybe"),
                Some(4100)
            ),
            Go::Settings {
                notifications: None,
                start: None,
                updates: None
            }
        );
        // Not from another site.
        let elsewhere = url("https://example.org/app/quit");
        assert_eq!(decide(&elsewhere, Some(4100)), Go::Outside(elsewhere));
    }
}
