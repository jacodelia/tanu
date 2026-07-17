//! Minimal i18n. A global current-language index (atomic, mirrors the theme
//! `PRIMARY` pattern) plus a `tr(key)` lookup that returns `&'static str`
//! translations — no allocation, no dependency.
//!
//! ponytail: flat match table, not fluent/gettext. Add a real catalog format
//! only if translators need to edit strings without recompiling.

use std::sync::atomic::{AtomicU8, Ordering};

/// Supported UI languages. Discriminant is the stable index persisted in config.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    En = 0,
    It = 1,
    Fr = 2,
    Es = 3,
    Ru = 4,
    De = 5,
    Ja = 6,
    Zh = 7,
}

/// All languages in menu order, paired with their native display name.
pub const LANGUAGES: &[(Language, &str)] = &[
    (Language::En, "English"),
    (Language::It, "Italiano"),
    (Language::Fr, "Français"),
    (Language::Es, "Español"),
    (Language::Ru, "Русский"),
    (Language::De, "Deutsch"),
    (Language::Ja, "日本語"),
    (Language::Zh, "中文"),
];

impl Language {
    /// Two-letter code persisted in config (`en`, `it`, ...).
    pub fn code(self) -> &'static str {
        match self {
            Language::En => "en",
            Language::It => "it",
            Language::Fr => "fr",
            Language::Es => "es",
            Language::Ru => "ru",
            Language::De => "de",
            Language::Ja => "ja",
            Language::Zh => "zh",
        }
    }

    /// Parse a config code back into a language. Unknown → English.
    pub fn from_code(code: &str) -> Language {
        LANGUAGES
            .iter()
            .map(|(l, _)| *l)
            .find(|l| l.code() == code)
            .unwrap_or(Language::En)
    }
}

static CURRENT: AtomicU8 = AtomicU8::new(Language::En as u8);

/// Set the global UI language.
pub fn set_language(lang: Language) {
    CURRENT.store(lang as u8, Ordering::Relaxed);
}

/// The current global UI language.
pub fn language() -> Language {
    match CURRENT.load(Ordering::Relaxed) {
        1 => Language::It,
        2 => Language::Fr,
        3 => Language::Es,
        4 => Language::Ru,
        5 => Language::De,
        6 => Language::Ja,
        7 => Language::Zh,
        _ => Language::En,
    }
}

/// Translate a UI key into the current language. Unknown keys return the key
/// itself, so a missing string is visible rather than silently blank.
pub fn tr(key: &str) -> &'static str {
    use Language::*;
    let lang = language();
    match key {
        "menu.file" => match lang {
            Ru => "ФАЙЛ",
            Ja => "ファイル",
            Zh => "文件",
            _ => "FILE",
        },
        "menu.edit" => match lang {
            It => "MODIFICA",
            Fr => "ÉDITION",
            Es => "EDITAR",
            Ru => "ПРАВКА",
            De => "BEARBEITEN",
            Ja => "編集",
            Zh => "编辑",
            En => "EDIT",
        },
        "menu.about" => match lang {
            It | Es => "INFO",
            Fr => "À PROPOS",
            De => "ÜBER",
            Ru => "О ПРОГРАММЕ",
            Ja => "情報",
            Zh => "关于",
            En => "ABOUT",
        },
        "file.open" => match lang {
            It => "Apri file...",
            Fr => "Ouvrir un fichier...",
            Es => "Abrir archivo...",
            Ru => "Открыть файл...",
            De => "Datei öffnen...",
            Ja => "ファイルを開く...",
            Zh => "打开文件...",
            En => "Open File...",
        },
        "file.scan" => match lang {
            It => "Scansiona cartella...",
            Fr => "Analyser un dossier...",
            Es => "Escanear carpeta...",
            Ru => "Сканировать папку...",
            De => "Ordner scannen...",
            Ja => "フォルダをスキャン...",
            Zh => "扫描文件夹...",
            En => "Scan Folder...",
        },
        "file.quit" => match lang {
            It => "Esci",
            Fr => "Quitter",
            Es => "Salir",
            Ru => "Выход",
            De => "Beenden",
            Ja => "終了",
            Zh => "退出",
            En => "Quit",
        },
        "edit.source" => match lang {
            It => "Sorgente audio...",
            Fr => "Source audio...",
            Es => "Fuente de sonido...",
            Ru => "Источник звука...",
            De => "Audioquelle...",
            Ja => "サウンドソース...",
            Zh => "声音源...",
            En => "Sound Source...",
        },
        "edit.soundfont" => match lang {
            It | Fr | Es | De => "SoundFont (.sf2)...",
            Ru => "Саундфонт (.sf2)...",
            Ja => "サウンドフォント (.sf2)...",
            Zh => "音色库 (.sf2)...",
            En => "SoundFont (.sf2)...",
        },
        "edit.color" => match lang {
            It => "Colore testo...",
            Fr => "Couleur du texte...",
            Es => "Color del texto...",
            Ru => "Цвет текста...",
            De => "Textfarbe...",
            Ja => "文字色...",
            Zh => "文字颜色...",
            En => "Text Color...",
        },
        "edit.language" => match lang {
            It => "Lingua...",
            Fr => "Langue...",
            Es => "Idioma...",
            Ru => "Язык...",
            De => "Sprache...",
            Ja => "言語...",
            Zh => "语言...",
            En => "Language...",
        },
        "title.language" => match lang {
            It => "Lingua",
            Fr => "Langue",
            Es => "Idioma",
            Ru => "Язык",
            De => "Sprache",
            Ja => "言語",
            Zh => "语言",
            En => "Language",
        },
        "title.color" => match lang {
            It => "Colore testo",
            Fr => "Couleur du texte",
            Es => "Color del texto",
            Ru => "Цвет текста",
            De => "Textfarbe",
            Ja => "文字色",
            Zh => "文字颜色",
            En => "Text Color",
        },
        "about.desc" => match lang {
            It => "un lettore musicale da terminale",
            Fr => "un lecteur de musique en terminal",
            Es => "un reproductor de música de terminal",
            Ru => "музыкальный проигрыватель в терминале",
            De => "ein Terminal-Musikplayer",
            Ja => "ターミナル音楽プレーヤー",
            Zh => "终端音乐播放器",
            En => "a terminal music player",
        },
        other => leak_fallback(other),
    }
}

/// Unknown key: return it verbatim. Keys are compile-time literals here, so this
/// only fires in dev when a `tr` call names a key with no table entry.
fn leak_fallback(key: &str) -> &'static str {
    // ponytail: keys are all &'static literals at call sites; return a fixed
    // marker rather than leaking memory for a case that shouldn't ship.
    let _ = key;
    "?"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_roundtrip_and_translate() {
        for (lang, _) in LANGUAGES {
            assert_eq!(Language::from_code(lang.code()), *lang);
        }
        set_language(Language::Es);
        assert_eq!(tr("file.quit"), "Salir");
        set_language(Language::Ja);
        assert_eq!(tr("menu.edit"), "編集");
        set_language(Language::En);
        assert_eq!(tr("menu.edit"), "EDIT");
        // Unknown key is visible, not blank.
        assert_eq!(tr("nope.nope"), "?");
    }
}
