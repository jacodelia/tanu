//! ReplayGain support — reads gain tags from audio files via lofty.

use std::path::Path;

use lofty::file::TaggedFileExt;

/// ReplayGain information extracted from a file.
#[derive(Debug, Clone, Default)]
pub struct ReplayGain {
    pub track_gain_db: Option<f32>,
    pub track_peak: Option<f32>,
    pub album_gain_db: Option<f32>,
    pub album_peak: Option<f32>,
}

/// Read ReplayGain tags from an audio file.
pub fn read_replaygain(path: &Path) -> Option<ReplayGain> {
    let tagged_file = lofty::read_from_path(path).ok()?;

    let tag = tagged_file.primary_tag()?;

    let mut rg = ReplayGain::default();

    for item in tag.items() {
        match item.key() {
            lofty::tag::ItemKey::ReplayGainTrackGain => {
                rg.track_gain_db = item.value().text().and_then(|s| {
                    s.trim().trim_end_matches(" dB").trim_end_matches("dB").parse().ok()
                });
            }
            lofty::tag::ItemKey::ReplayGainTrackPeak => {
                rg.track_peak = item.value().text().and_then(|s| s.trim().parse().ok());
            }
            lofty::tag::ItemKey::ReplayGainAlbumGain => {
                rg.album_gain_db = item.value().text().and_then(|s| {
                    s.trim().trim_end_matches(" dB").trim_end_matches("dB").parse().ok()
                });
            }
            lofty::tag::ItemKey::ReplayGainAlbumPeak => {
                rg.album_peak = item.value().text().and_then(|s| s.trim().parse().ok());
            }
            _ => {}
        }
    }

    if rg.track_gain_db.is_some() || rg.album_gain_db.is_some() {
        Some(rg)
    } else {
        None
    }
}
