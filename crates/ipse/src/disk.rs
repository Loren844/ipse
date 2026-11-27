//! Whether the disk that holds the memory is encrypted (issue #262, decision 0009): the
//! vault's files are plain Markdown, and their protection at rest is the system's disk
//! encryption. Ipse only reads the state and explains; it changes nothing.
//!
//! - macOS: `FileVault`, from `fdesetup status`.
//! - Windows: `BitLocker` or device encryption on the memory's drive, from the shell's
//!   `System.Volume.BitLockerProtection` property (no administrator rights needed),
//!   else `manage-bde -status`.
//! - Linux: whether the memory's folder sits on a dm-crypt (LUKS) device, directly or
//!   under LVM, or on an `eCryptfs` home.
//!
//! When it cannot tell, the state is unknown, never a guess.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Whether the memory's disk is encrypted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Encryption {
    On,
    Off,
    /// Ipse could not tell.
    Unknown,
}

impl Encryption {
    /// `on`, `off` or `unknown`, as the interface receives it.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::On => "on",
            Self::Off => "off",
            Self::Unknown => "unknown",
        }
    }
}

/// The system Ipse runs on, for the steps the interface gives: `macos`, `windows`,
/// `linux` or `other`.
pub(crate) fn system() -> &'static str {
    match std::env::consts::OS {
        os @ ("macos" | "windows" | "linux") => os,
        _ => "other",
    }
}

/// How long a state read is kept: the system's tools are slow to start (`PowerShell`
/// takes about a second), and encryption is not turned on every minute.
const KEEP: Duration = Duration::from_mins(10);

/// The longest a system tool may take before the state is unknown.
#[cfg_attr(not(any(target_os = "macos", windows, test)), allow(dead_code))]
const PATIENCE: Duration = Duration::from_secs(8);

static LAST: Mutex<Option<(PathBuf, Instant, Encryption)>> = Mutex::new(None);

/// The encryption of the disk holding `folder`, read again after [`KEEP`].
/// `IPSE_DISK_ENCRYPTION` (`on`, `off` or `unknown`) stands for the system's answer,
/// for the interface's tests.
pub(crate) fn of(folder: &Path) -> Encryption {
    match std::env::var("IPSE_DISK_ENCRYPTION").as_deref() {
        Ok("on") => return Encryption::On,
        Ok("off") => return Encryption::Off,
        Ok("unknown") => return Encryption::Unknown,
        _ => {}
    }
    if let Ok(last) = LAST.lock()
        && let Some((path, at, state)) = last.as_ref()
        && path == folder
        && at.elapsed() < KEEP
    {
        return *state;
    }
    let state = read(folder);
    if let Ok(mut last) = LAST.lock() {
        *last = Some((folder.to_owned(), Instant::now(), state));
    }
    state
}

/// What `ipse check` and `ipse init` say about it, in English as the command line.
pub(crate) fn sentence(state: Encryption) -> &'static str {
    match (state, system()) {
        (Encryption::On, "macos") => "The disk is encrypted (FileVault is on).",
        (Encryption::On, "windows") => "The drive is encrypted (BitLocker is on).",
        (Encryption::On, _) => "The disk is encrypted.",
        (Encryption::Off, "macos") => {
            "The disk is not encrypted: anyone with the computer could read your memory. Turn on FileVault in System Settings > Privacy & Security > FileVault, and keep the recovery key somewhere safe."
        }
        (Encryption::Off, "windows") => {
            "The drive is not encrypted: anyone with the computer could read your memory. Turn on Device encryption in Settings > Privacy & security, or BitLocker (Control Panel > BitLocker Drive Encryption) on Windows Pro, and keep the recovery key somewhere safe."
        }
        (Encryption::Off, _) => {
            "The disk is not encrypted: anyone with the computer could read your memory. Most distributions offer to encrypt the disk (LUKS) when they are installed; on a system already installed, keep your memory on an encrypted partition or reinstall with that option, after a backup."
        }
        (Encryption::Unknown, _) => {
            "Ipse could not tell whether the disk is encrypted. Check it in your system's settings: FileVault on a Mac, BitLocker on Windows, LUKS on Linux."
        }
    }
}

#[cfg(target_os = "macos")]
fn read(_folder: &Path) -> Encryption {
    output("fdesetup", &["status"]).map_or(Encryption::Unknown, |o| fdesetup(&o))
}

#[cfg(windows)]
fn read(folder: &Path) -> Encryption {
    let Some(drive) = drive_of(folder) else {
        return Encryption::Unknown;
    };
    let script = format!(
        "(New-Object -ComObject Shell.Application).NameSpace('{drive}').Self.ExtendedProperty('System.Volume.BitLockerProtection')"
    );
    let shell = output(
        "powershell",
        &["-NoProfile", "-NonInteractive", "-Command", &script],
    )
    .map_or(Encryption::Unknown, |o| bitlocker_property(&o));
    if shell != Encryption::Unknown {
        return shell;
    }
    output("manage-bde", &["-status", &drive]).map_or(Encryption::Unknown, |o| manage_bde(&o))
}

/// `C:` for a folder on that drive; `None` for a network share.
#[cfg(windows)]
fn drive_of(folder: &Path) -> Option<String> {
    use std::path::{Component, Prefix};
    let full = std::fs::canonicalize(folder).ok()?;
    match full.components().next()? {
        Component::Prefix(p) => match p.kind() {
            Prefix::Disk(letter) | Prefix::VerbatimDisk(letter) => {
                Some(format!("{}:", char::from(letter)))
            }
            _ => None,
        },
        _ => None,
    }
}

#[cfg(target_os = "linux")]
fn read(folder: &Path) -> Encryption {
    let Ok(full) = std::fs::canonicalize(folder) else {
        return Encryption::Unknown;
    };
    let Ok(mountinfo) = std::fs::read_to_string("/proc/self/mountinfo") else {
        return Encryption::Unknown;
    };
    let Some(mount) = mount_of(&mountinfo, &full) else {
        return Encryption::Unknown;
    };
    linux_state(&mount, Path::new("/sys/dev/block"), |source| {
        use std::os::unix::fs::MetadataExt as _;
        let rdev = std::fs::metadata(source).ok()?.rdev();
        // Linux's encoding of a device number.
        let major = ((rdev >> 8) & 0xfff) | ((rdev >> 32) & !0xfff);
        let minor = (rdev & 0xff) | ((rdev >> 12) & !0xff);
        Some((u32::try_from(major).ok()?, u32::try_from(minor).ok()?))
    })
}

#[cfg(not(any(target_os = "macos", windows, target_os = "linux")))]
fn read(_folder: &Path) -> Encryption {
    Encryption::Unknown
}

/// What a system tool printed, if it ran and succeeded within [`PATIENCE`].
#[cfg(any(target_os = "macos", windows))]
fn output(program: &str, args: &[&str]) -> Option<String> {
    use std::process::{Command, Stdio};
    let mut command = Command::new(program);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        /// `CREATE_NO_WINDOW`: no console flashes open behind the app.
        const NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(NO_WINDOW);
    }
    let mut child = command.spawn().ok()?;
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let mut out = String::new();
                std::io::Read::read_to_string(&mut child.stdout.take()?, &mut out).ok()?;
                return status.success().then_some(out);
            }
            Ok(None) if started.elapsed() < PATIENCE => {
                std::thread::sleep(Duration::from_millis(20));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
}

/// `fdesetup status`: `FileVault is On.`, `FileVault is Off.`, or encryption under
/// way (`Encryption in progress`), which already counts as on.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn fdesetup(out: &str) -> Encryption {
    if out.contains("FileVault is On") || out.contains("Encryption in progress") {
        Encryption::On
    } else if out.contains("FileVault is Off") || out.contains("Decryption in progress") {
        Encryption::Off
    } else {
        Encryption::Unknown
    }
}

/// The shell's `System.Volume.BitLockerProtection` of a drive: 1 on, 3 encrypting, 6
/// on and locked; 2 off, 4 decrypting, 5 suspended (the key is left in clear); nothing
/// or 0 for a drive that cannot be encrypted, or when it cannot tell.
#[cfg_attr(not(windows), allow(dead_code))]
fn bitlocker_property(out: &str) -> Encryption {
    match out.trim() {
        "1" | "3" | "6" => Encryption::On,
        "2" | "4" | "5" => Encryption::Off,
        _ => Encryption::Unknown,
    }
}

/// `manage-bde -status C:`, in English: its `Protection Status`. Other languages, or
/// no rights to run it, are unknown.
#[cfg_attr(not(windows), allow(dead_code))]
fn manage_bde(out: &str) -> Encryption {
    let status = out
        .lines()
        .find_map(|l| l.trim().strip_prefix("Protection Status:"))
        .map(str::trim);
    match status {
        Some(s) if s.starts_with("Protection On") => Encryption::On,
        Some(s) if s.starts_with("Protection Off") => Encryption::Off,
        _ => Encryption::Unknown,
    }
}

/// The mount holding a folder, from `/proc/self/mountinfo`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
struct Mount {
    /// The device's major and minor numbers; 0 for a virtual device (btrfs, a
    /// network file system).
    device: (u32, u32),
    fs_type: String,
    source: String,
}

/// The mount of `/proc/self/mountinfo` with the longest mount point holding `path`
/// (absolute, symbolic links resolved).
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn mount_of(mountinfo: &str, path: &Path) -> Option<Mount> {
    let mut best: Option<(usize, Mount)> = None;
    for line in mountinfo.lines() {
        // id parent major:minor root mount-point options [optional…] - type source super
        let Some((before, after)) = line.split_once(" - ") else {
            continue;
        };
        let fields: Vec<&str> = before.split(' ').collect();
        let (Some(device), Some(point)) = (fields.get(2), fields.get(4)) else {
            continue;
        };
        let point = unescape(point);
        if !path.starts_with(&point) {
            continue;
        }
        let depth = Path::new(&point).components().count();
        if best.as_ref().is_some_and(|(d, _)| *d > depth) {
            continue;
        }
        let mut rest = after.split(' ');
        let fs_type = rest.next().unwrap_or_default().to_owned();
        let source = unescape(rest.next().unwrap_or_default());
        let Some((Ok(major), Ok(minor))) = device
            .split_once(':')
            .map(|(a, b)| (a.parse::<u32>(), b.parse::<u32>()))
        else {
            continue;
        };
        best = Some((
            depth,
            Mount {
                device: (major, minor),
                fs_type,
                source,
            },
        ));
    }
    best.map(|(_, m)| m)
}

/// mountinfo writes a space as `\040`, a tab `\011`, a backslash `\134`.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn unescape(text: &str) -> String {
    text.replace("\\040", " ")
        .replace("\\011", "\t")
        .replace("\\012", "\n")
        .replace("\\134", "\\")
}

/// The encryption of a Linux mount, its block devices read from `sys` (normally
/// `/sys/dev/block`); `device_of` gives the numbers of a device file, for a file system
/// on a virtual device (btrfs) whose source is a real one.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn linux_state(
    mount: &Mount,
    sys: &Path,
    device_of: impl Fn(&Path) -> Option<(u32, u32)>,
) -> Encryption {
    if mount.fs_type == "ecryptfs" {
        return Encryption::On;
    }
    let device = if mount.device.0 == 0 {
        if !mount.source.starts_with("/dev/") {
            // tmpfs, overlay (a container), a network or FUSE file system.
            return Encryption::Unknown;
        }
        match device_of(Path::new(&mount.source)) {
            Some(d) => d,
            None => return Encryption::Unknown,
        }
    } else {
        mount.device
    };
    let block = sys.join(format!("{}:{}", device.0, device.1));
    if !block.exists() {
        return Encryption::Unknown;
    }
    if crypt_below(&block, 0) {
        Encryption::On
    } else {
        Encryption::Off
    }
}

/// True when `block`, or a device it is built on (LVM over LUKS), is a dm-crypt one.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn crypt_below(block: &Path, depth: usize) -> bool {
    if depth > 8 {
        return false;
    }
    let uuid = std::fs::read_to_string(block.join("dm/uuid")).unwrap_or_default();
    if uuid.starts_with("CRYPT-") {
        return true;
    }
    let Ok(slaves) = std::fs::read_dir(block.join("slaves")) else {
        return false;
    };
    slaves.flatten().any(|s| crypt_below(&s.path(), depth + 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_filevault() {
        // Recorded on macOS 15.
        assert_eq!(fdesetup("FileVault is On.\n"), Encryption::On);
        assert_eq!(fdesetup("FileVault is Off.\n"), Encryption::Off);
        assert_eq!(
            fdesetup("Encryption in progress: Percent completed = 12.5\n"),
            Encryption::On
        );
        assert_eq!(
            fdesetup("FileVault is On.\nFileVault master keychain appears to be installed.\n"),
            Encryption::On
        );
        assert_eq!(fdesetup(""), Encryption::Unknown);
    }

    #[test]
    fn reads_bitlocker() {
        assert_eq!(bitlocker_property("1\r\n"), Encryption::On);
        assert_eq!(bitlocker_property("2\r\n"), Encryption::Off);
        assert_eq!(bitlocker_property("5\r\n"), Encryption::Off, "suspended");
        assert_eq!(bitlocker_property("\r\n"), Encryption::Unknown);
        assert_eq!(bitlocker_property("0"), Encryption::Unknown);

        // Recorded from `manage-bde -status C:` on Windows 11.
        let on = "BitLocker Drive Encryption: Configuration Tool version 10.0.22621\r\nCopyright (C) 2013 Microsoft Corporation. All rights reserved.\r\n\r\nVolume C: [Windows]\r\n[OS Volume]\r\n\r\n    Size:                 475,95 GB\r\n    BitLocker Version:    2.0\r\n    Conversion Status:    Used Space Only Encrypted\r\n    Percentage Encrypted: 100,0%\r\n    Encryption Method:    XTS-AES 128\r\n    Protection Status:    Protection On\r\n    Lock Status:          Unlocked\r\n";
        assert_eq!(manage_bde(on), Encryption::On);
        assert_eq!(
            manage_bde(&on.replace("Protection On", "Protection Off")),
            Encryption::Off
        );
        assert_eq!(
            manage_bde("Statut de la protection : Protection activée"),
            Encryption::Unknown,
            "another language"
        );
    }

    /// Recorded on Ubuntu 24.04 installed with "Encrypt the disk" (LUKS, then LVM),
    /// with a USB stick and a container's overlay added.
    const MOUNTINFO: &str = "\
22 1 252:1 / / rw,relatime shared:1 - ext4 /dev/mapper/ubuntu--vg-ubuntu--lv rw
25 22 259:2 / /boot rw,relatime shared:2 - ext4 /dev/nvme0n1p2 rw
26 22 0:24 / /tmp rw,nosuid shared:3 - tmpfs tmpfs rw
30 22 8:17 / /media/camille/Cl\\040USB rw,nosuid shared:4 - exfat /dev/sdb1 rw
31 22 0:40 / /home/camille/.Private rw shared:5 - ecryptfs /home/camille/.Private rw
32 22 0:41 / /srv/btrfs rw shared:6 - btrfs /dev/mapper/luks-1234 rw
";

    #[test]
    fn finds_the_mount_of_a_folder() {
        let mount = |p: &str| mount_of(MOUNTINFO, Path::new(p)).unwrap();
        assert_eq!(mount("/home/camille/Ipse").device, (252, 1));
        assert_eq!(mount("/boot/efi").source, "/dev/nvme0n1p2");
        assert_eq!(
            mount("/media/camille/Cl USB/Ipse").source,
            "/dev/sdb1",
            "a space in the mount point"
        );
        assert_eq!(mount("/tmp/x").fs_type, "tmpfs");
    }

    /// A fake `/sys/dev/block`, whose folders are named `major:minor` as on Linux: a
    /// name Windows refuses, so these tests run where such names exist. 252:1 an LVM volume on 252:0, a LUKS device on the
    /// disk's partition 259:3; 8:17 a plain USB stick; 252:5 a LUKS device for btrfs.
    #[cfg(unix)]
    fn sys() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let block = |name: &str, uuid: Option<&str>, slaves: &[&str]| {
            let at = dir.path().join(name);
            std::fs::create_dir_all(at.join("slaves")).unwrap();
            if let Some(uuid) = uuid {
                std::fs::create_dir_all(at.join("dm")).unwrap();
                std::fs::write(at.join("dm/uuid"), uuid).unwrap();
            }
            for slave in slaves {
                std::fs::create_dir_all(at.join("slaves").join(slave)).unwrap();
                // A slave is a link to the device's own folder; a copy reads the same.
                let target = dir.path().join(slave);
                if target.join("dm/uuid").exists() {
                    std::fs::create_dir_all(at.join("slaves").join(slave).join("dm")).unwrap();
                    std::fs::copy(
                        target.join("dm/uuid"),
                        at.join("slaves").join(slave).join("dm/uuid"),
                    )
                    .unwrap();
                }
            }
        };
        block("259:3", None, &[]);
        block("252:0", Some("CRYPT-LUKS2-0123abcd-dm_crypt-0"), &["259:3"]);
        block("252:1", Some("LVM-AbCdEf"), &["252:0"]);
        block("8:17", None, &[]);
        block("252:5", Some("CRYPT-LUKS2-4567-luks-1234"), &[]);
        dir
    }

    #[test]
    #[cfg(unix)]
    fn tells_whether_a_linux_folder_is_on_luks() {
        let sys = sys();
        let state = |p: &str| {
            let mount = mount_of(MOUNTINFO, Path::new(p)).unwrap();
            linux_state(&mount, sys.path(), |source| {
                (source == Path::new("/dev/mapper/luks-1234")).then_some((252, 5))
            })
        };
        assert_eq!(state("/home/camille/Ipse"), Encryption::On, "LVM on LUKS");
        assert_eq!(state("/media/camille/Cl USB/Ipse"), Encryption::Off);
        assert_eq!(
            state("/boot"),
            Encryption::Unknown,
            "a device sys does not list"
        );
        assert_eq!(state("/tmp/Ipse"), Encryption::Unknown, "tmpfs");
        assert_eq!(
            state("/home/camille/.Private/x"),
            Encryption::On,
            "eCryptfs"
        );
        assert_eq!(state("/srv/btrfs/Ipse"), Encryption::On, "btrfs on LUKS");
    }

    #[test]
    fn reads_this_computer_without_failing() {
        // On each system of the CI: whatever the state, it is read, quickly enough.
        let dir = tempfile::tempdir().unwrap();
        let started = Instant::now();
        let state = of(dir.path());
        println!("disk encryption here: {}", state.as_str());
        assert!(started.elapsed() < PATIENCE * 2 + Duration::from_secs(1));
        assert_eq!(of(dir.path()), state, "kept");
        assert!(!sentence(state).is_empty());
    }
}
