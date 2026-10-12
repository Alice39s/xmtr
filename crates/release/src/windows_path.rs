use std::io;
use std::path::Path;

use anyhow::{Context, Result};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    HWND_BROADCAST, SMTO_ABORTIFHUNG, SendMessageTimeoutW, WM_SETTINGCHANGE,
};
use winreg::RegKey;
use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_WRITE, RegType};

use crate::path_list;

fn lookup(name: &str) -> Option<String> {
    std::env::var(name).ok()
}

/// Rewrites the user PATH in the registry when `edit` changes it, then tells running programs.
/// A PATH that cannot be read is an error, so it is never replaced by a list that lost its entries.
fn edit_user_path(edit: impl FnOnce(&[&str]) -> Option<Vec<String>>) -> Result<bool> {
    let environment = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags("Environment", KEY_READ | KEY_WRITE)
        .context("opening HKCU\\Environment")?;
    let current: String = match environment.get_value("Path") {
        Ok(value) => value,
        Err(error) if error.kind() == io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(error).context("reading the user PATH"),
    };
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
    edit_user_path(|entries| path_list::with_dir(entries, &dir, lookup))
}

/// Removes `dir` from the user PATH; returns false when it was not there.
pub fn remove_from_user_path(dir: &Path) -> Result<bool> {
    let dir = dir.to_string_lossy();
    edit_user_path(|entries| path_list::without_dir(entries, &dir, lookup))
}
