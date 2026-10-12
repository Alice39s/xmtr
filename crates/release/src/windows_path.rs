use std::path::Path;

use anyhow::{Context, Result};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    HWND_BROADCAST, SMTO_ABORTIFHUNG, SendMessageTimeoutW, WM_SETTINGCHANGE,
};
use winreg::RegKey;
use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_WRITE, RegType};

fn same_dir(entry: &str, dir: &str) -> bool {
    entry
        .trim_end_matches('\\')
        .eq_ignore_ascii_case(dir.trim_end_matches('\\'))
}

/// Rewrites the user PATH in the registry when `edit` changes it, then tells running programs.
fn edit_user_path(edit: impl FnOnce(&[&str]) -> Option<Vec<String>>) -> Result<bool> {
    let environment = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags("Environment", KEY_READ | KEY_WRITE)
        .context("opening HKCU\\Environment")?;
    let current: String = environment.get_value("Path").unwrap_or_default();
    let entries: Vec<&str> = current
        .split(';')
        .filter(|entry| !entry.is_empty())
        .collect();
    let Some(updated) = edit(&entries) else {
        return Ok(false);
    };
    let joined = updated.join(";");
    let mut value = winreg::types::ToRegValue::to_reg_value(&joined);
    value.vtype = RegType::REG_EXPAND_SZ;
    environment
        .set_raw_value("Path", &value)
        .context("writing the user PATH")?;
    let name: Vec<u16> = "Environment\0".encode_utf16().collect();
    let mut result = 0usize;
    // SAFETY: `name` is a NUL-terminated UTF-16 string that outlives the call; the result pointer is valid.
    unsafe {
        SendMessageTimeoutW(
            HWND_BROADCAST,
            WM_SETTINGCHANGE,
            0,
            name.as_ptr() as isize,
            SMTO_ABORTIFHUNG,
            5000,
            &mut result,
        );
    }
    Ok(true)
}

/// Appends `dir` to the user PATH; returns false when it is already there.
pub fn add_to_user_path(dir: &Path) -> Result<bool> {
    let dir = dir.to_string_lossy();
    edit_user_path(|entries| {
        if entries.iter().any(|entry| same_dir(entry, &dir)) {
            return None;
        }
        let mut updated: Vec<String> = entries.iter().map(|entry| (*entry).to_owned()).collect();
        updated.push(dir.into_owned());
        Some(updated)
    })
}

/// Removes `dir` from the user PATH; returns false when it was not there.
pub fn remove_from_user_path(dir: &Path) -> Result<bool> {
    let dir = dir.to_string_lossy();
    edit_user_path(|entries| {
        let kept: Vec<String> = entries
            .iter()
            .filter(|entry| !same_dir(entry, &dir))
            .map(|entry| (*entry).to_owned())
            .collect();
        (kept.len() != entries.len()).then_some(kept)
    })
}
