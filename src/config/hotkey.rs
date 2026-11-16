//! Parses and formats the global hotkey text saved in `config.toml`.
//!
//! Registration with Windows lives in the tray; this module only validates text.

use anyhow::{Context, Result, bail, ensure};

/// Default global hotkey for Native ↔ Stretch toggle.
pub const DEFAULT_HOTKEY: &str = "Ctrl+Alt+S";

/// Modifier bits matching Win32 `MOD_*` (`HOT_KEY_MODIFIERS`).
pub const MOD_ALT: u32 = 0x0001;
pub const MOD_CONTROL: u32 = 0x0002;
pub const MOD_SHIFT: u32 = 0x0004;
pub const MOD_WIN: u32 = 0x0008;

/// Parses strings like `Ctrl+Alt+S` into `(modifiers, virtual-key code)`.
///
/// Modifiers (case-insensitive): `Ctrl`/`Control`, `Alt`, `Shift`,
/// `Win`/`Windows`/`Super`/`Meta`. Exactly one main key is required:
/// `A`–`Z`, `0`–`9`, `F1`–`F24`, `Space`, `Tab`, `Enter`, `Esc`/`Escape`,
/// `Backspace`, `Delete`, `Insert`, `Home`, `End`, `PageUp`, `PageDown`,
/// `Up`, `Down`, `Left`, `Right`. At least one modifier is required so a
/// plain key is never hijacked globally. Empty strings are rejected here;
/// callers treat empty as disabled.
pub fn parse(text: &str) -> Result<(u32, u32)> {
    let trimmed = text.trim();
    ensure!(
        !trimmed.is_empty(),
        "hotkey is empty (e.g. {DEFAULT_HOTKEY})"
    );
    let mut modifiers = 0u32;
    let mut key: Option<u32> = None;
    for raw in trimmed.split('+') {
        let token = raw.trim();
        ensure!(!token.is_empty(), "invalid hotkey '{text}': empty part");
        let lower = token.to_ascii_lowercase();
        let modifier = match lower.as_str() {
            "ctrl" | "control" => Some(MOD_CONTROL),
            "alt" => Some(MOD_ALT),
            "shift" => Some(MOD_SHIFT),
            "win" | "windows" | "super" | "meta" => Some(MOD_WIN),
            _ => None,
        };
        if let Some(bit) = modifier {
            modifiers |= bit;
            continue;
        }
        let vk = key_to_vk(&lower)
            .with_context(|| format!("invalid hotkey '{text}': unknown key '{token}'"))?;
        if key.is_some() {
            bail!("invalid hotkey '{text}': only one main key is supported");
        }
        key = Some(vk);
    }
    ensure!(
        modifiers != 0,
        "invalid hotkey '{text}': include at least one of Ctrl, Alt, Shift, Win"
    );
    let vk = key.context(format!(
        "invalid hotkey '{text}': missing a main key (e.g. {DEFAULT_HOTKEY})"
    ))?;
    Ok((modifiers, vk))
}

fn key_to_vk(lower: &str) -> Result<u32> {
    if lower.len() == 1 {
        let c = lower.chars().next().unwrap_or_default();
        if c.is_ascii_alphabetic() {
            return Ok(c.to_ascii_uppercase() as u32);
        }
        if c.is_ascii_digit() {
            return Ok(c as u32);
        }
        bail!("unsupported key '{lower}': use A-Z, 0-9, F1-F24, or a named key");
    }
    if let Some(number) = lower.strip_prefix('f')
        && let Ok(n) = number.parse::<u32>()
        && (1..=24).contains(&n)
    {
        return Ok(0x70 + n - 1);
    }
    let vk = match lower {
        "space" => 0x20,
        "tab" => 0x09,
        "enter" | "return" => 0x0D,
        "esc" | "escape" => 0x1B,
        "backspace" => 0x08,
        "delete" | "del" => 0x2E,
        "insert" | "ins" => 0x2D,
        "home" => 0x24,
        "end" => 0x23,
        "pageup" | "pgup" => 0x21,
        "pagedown" | "pgdn" => 0x22,
        "up" => 0x26,
        "down" => 0x28,
        "left" => 0x25,
        "right" => 0x27,
        _ => bail!("unsupported key '{lower}': use A-Z, 0-9, F1-F24, or a named key"),
    };
    Ok(vk)
}

/// Normalized `Ctrl+Alt+S` style label for menus and errors.
pub fn canonical(text: &str) -> Result<String> {
    let (modifiers, vk) = parse(text)?;
    let mut parts = Vec::new();
    if modifiers & MOD_CONTROL != 0 {
        parts.push("Ctrl");
    }
    if modifiers & MOD_ALT != 0 {
        parts.push("Alt");
    }
    if modifiers & MOD_SHIFT != 0 {
        parts.push("Shift");
    }
    if modifiers & MOD_WIN != 0 {
        parts.push("Win");
    }
    let key = if (0x41..=0x5A).contains(&vk) || (0x30..=0x39).contains(&vk) {
        (vk as u8 as char).to_string()
    } else if (0x70..=0x87).contains(&vk) {
        format!("F{}", vk - 0x70 + 1)
    } else {
        match vk {
            0x20 => "Space".into(),
            0x09 => "Tab".into(),
            0x0D => "Enter".into(),
            0x1B => "Esc".into(),
            0x08 => "Backspace".into(),
            0x2E => "Delete".into(),
            0x2D => "Insert".into(),
            0x24 => "Home".into(),
            0x23 => "End".into(),
            0x21 => "PageUp".into(),
            0x22 => "PageDown".into(),
            0x26 => "Up".into(),
            0x28 => "Down".into(),
            0x25 => "Left".into(),
            0x27 => "Right".into(),
            _ => format!("VK{vk:#X}"),
        }
    };
    parts.push(&key);
    Ok(parts.join("+"))
}

pub(super) fn validate(hotkey: &str) -> Result<()> {
    // Empty clears the hotkey; the tray treats it as disabled.
    if hotkey.trim().is_empty() {
        return Ok(());
    }
    parse(hotkey)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    #[test]
    fn hotkey_defaults_to_ctrl_alt_s_and_enabled() {
        let c = Config::default();
        assert_eq!(c.hotkey, "Ctrl+Alt+S");
        assert!(c.hotkey_enabled);
        c.validate().unwrap();
        let (mods, vk) = parse(&c.hotkey).unwrap();
        assert_eq!(mods, MOD_CONTROL | MOD_ALT);
        assert_eq!(vk, u32::from(b'S'));
        assert_eq!(canonical("ctrl+alt+s").unwrap(), "Ctrl+Alt+S");
    }

    #[test]
    fn hotkey_parsing_accepts_common_forms() {
        assert_eq!(
            parse("Ctrl+Shift+F12").unwrap(),
            (MOD_CONTROL | MOD_SHIFT, 0x7B)
        );
        assert_eq!(canonical("alt+ctrl+s").unwrap(), "Ctrl+Alt+S");
        assert_eq!(canonical("Win+Alt+Space").unwrap(), "Alt+Win+Space");
        // Canonical order is Ctrl, Alt, Shift, Win.
        assert_eq!(canonical("Shift+Win+Ctrl+A").unwrap(), "Ctrl+Shift+Win+A");
        assert_eq!(parse("ctrl+alt+1").unwrap().1, u32::from(b'1'));
    }

    #[test]
    fn hotkey_parsing_rejects_invalid_combos() {
        for invalid in [
            "",
            "   ",
            "Ctrl+Alt",
            "S",
            "Ctrl+Alt+S+D",
            "Ctrl+Foo+S",
            "Ctrl++S",
        ] {
            assert!(parse(invalid).is_err(), "{invalid}");
        }
        // Modifier-only and key-only combos must not hijack plain keys.
        assert!(parse("Ctrl").is_err());
        assert!(parse("A").is_err());
    }
}
