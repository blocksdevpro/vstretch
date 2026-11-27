//! Built-in stretch resolutions shared by the tray and terminal pickers.

/// A built-in stretch preset.
pub struct PopularPreset {
    pub width: u32,
    pub height: u32,
    pub aspect: &'static str,
    /// An optional label shown beside the resolution in the terminal picker.
    pub tag: Option<&'static str>,
}

impl PopularPreset {
    /// Pads the dimensions so resolution columns line up in the picker.
    pub fn resolution_label(&self) -> String {
        format!("{:>4} × {:<4}", self.width, self.height)
    }

    #[cfg(test)]
    fn name(&self) -> String {
        format!("{}x{}", self.width, self.height)
    }
}

/// Built-in stretch resolutions, sorted by descending height, then width.
/// Exactly one `common` and one `popular` tag in the whole list.
pub const POPULAR_STRETCH: &[PopularPreset] = &[
    PopularPreset {
        width: 1920,
        height: 1440,
        aspect: "4:3",
        tag: None,
    },
    PopularPreset {
        width: 1920,
        height: 1200,
        aspect: "16:10",
        tag: None,
    },
    PopularPreset {
        width: 1600,
        height: 1200,
        aspect: "4:3",
        tag: None,
    },
    PopularPreset {
        width: 1728,
        height: 1080,
        aspect: "16:10",
        tag: None,
    },
    PopularPreset {
        width: 1440,
        height: 1080,
        aspect: "4:3",
        tag: Some("popular"),
    },
    PopularPreset {
        width: 1680,
        height: 1050,
        aspect: "16:10",
        tag: None,
    },
    PopularPreset {
        width: 1280,
        height: 1024,
        aspect: "5:4",
        tag: None,
    },
    PopularPreset {
        width: 1280,
        height: 960,
        aspect: "4:3",
        tag: Some("common"),
    },
    PopularPreset {
        width: 1440,
        height: 900,
        aspect: "16:10",
        tag: None,
    },
    PopularPreset {
        width: 1152,
        height: 864,
        aspect: "4:3",
        tag: None,
    },
    PopularPreset {
        width: 1024,
        height: 768,
        aspect: "4:3",
        tag: None,
    },
];

impl PopularPreset {
    #[cfg(test)]
    fn find(query: &str) -> Option<&'static PopularPreset> {
        let q = query.trim().to_lowercase().replace('×', "x");
        POPULAR_STRETCH
            .iter()
            .find(|p| p.name().eq_ignore_ascii_case(&q))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_popular_preset() {
        let p = PopularPreset::find("1440x1080").unwrap();
        assert_eq!((p.width, p.height), (1440, 1080));
        assert!(PopularPreset::find("9999x9999").is_none());
    }
}
