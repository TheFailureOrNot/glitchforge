use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug)]
pub struct EffectInfo {
    pub id: &'static str,
    pub label: &'static str,
    pub hint: &'static str,
}

#[derive(Clone, Copy, Debug)]
pub struct PresetInfo {
    pub id: &'static str,
    pub label: &'static str,
    pub description: &'static str,
}

pub const REALISTIC_EFFECTS: [EffectInfo; 19] = [
    EffectInfo {
        id: "edge_corruption",
        label: "Edge corruption",
        hint: "Coloured memory-garbage strips and edge fragments",
    },
    EffectInfo {
        id: "block_displace",
        label: "Block displacement",
        hint: "Displaced macroblocks and motion-vector errors",
    },
    EffectInfo {
        id: "slice_shift",
        label: "Slice shift",
        hint: "Horizontal packet tears and channel desynchronisation",
    },
    EffectInfo {
        id: "stall_smear",
        label: "Buffer stall / smear",
        hint: "Repeated rows and columns from a stalled buffer",
    },
    EffectInfo {
        id: "channel_shift",
        label: "RGB channel shift",
        hint: "Red and blue plane misregistration",
    },
    EffectInfo {
        id: "chroma_bleed",
        label: "Chroma bleed",
        hint: "Chroma-plane desynchronisation and coarse subsampling",
    },
    EffectInfo {
        id: "macroblock_decay",
        label: "Macroblock decay",
        hint: "Local low-resolution blocks and coarse quantisation",
    },
    EffectInfo {
        id: "bit_flip",
        label: "Bit flips",
        hint: "Sparse clustered pixel-buffer bit flips",
    },
    EffectInfo {
        id: "stride_drift",
        label: "Stride drift",
        hint: "Row-group shifts from a misaligned pixel stride",
    },
    EffectInfo {
        id: "jpeg_databend",
        label: "JPEG artifacts",
        hint: "JPEG quantisation and chroma subsampling simulation",
    },
    EffectInfo {
        id: "dropout_blocks",
        label: "Decoder dropouts",
        hint: "Missing blocks filled with black, neutral or local colour",
    },
    EffectInfo {
        id: "streak_lines",
        label: "Signal streaks",
        hint: "Short bright or chromatic horizontal streaks",
    },
    EffectInfo {
        id: "ghost",
        label: "Frame ghosting",
        hint: "Faint displaced frame echoes in horizontal bands",
    },
    EffectInfo {
        id: "scanline_noise",
        label: "Scanline noise",
        hint: "Analog-style row luminance fluctuations",
    },
    EffectInfo {
        id: "color_banding",
        label: "Colour banding",
        hint: "Local coarse colour quantisation",
    },
    EffectInfo {
        id: "chromatic_aberration",
        label: "Chromatic aberration",
        hint: "Radially increasing red / blue edge separation",
    },
    EffectInfo {
        id: "analog_noise",
        label: "Analog noise",
        hint: "Correlated luma noise and softer chroma grain",
    },
    EffectInfo {
        id: "rolling_shutter",
        label: "Rolling shutter",
        hint: "Slowly varying row displacement",
    },
    EffectInfo {
        id: "interlace_artifact",
        label: "Interlace mismatch",
        hint: "Alternating scanline field displacement",
    },
];

pub const ARTISTIC_EFFECTS: [EffectInfo; 8] = [
    EffectInfo {
        id: "art_duotone",
        label: "Crimson duotone",
        hint: "Deep-ink shadows and saturated crimson highlights",
    },
    EffectInfo {
        id: "art_wave_warp",
        label: "Wave / ripple warp",
        hint: "Flowing scan-synchronous distortion",
    },
    EffectInfo {
        id: "art_halftone",
        label: "Halftone print",
        hint: "Resolution-aware circular print-screen texture",
    },
    EffectInfo {
        id: "art_scanlines",
        label: "Graphic scanlines",
        hint: "Deliberate CRT bands and vertical interference",
    },
    EffectInfo {
        id: "art_light_streaks",
        label: "Luminous streaks",
        hint: "Sharp red / white bars with a soft glow",
    },
    EffectInfo {
        id: "art_neon_edges",
        label: "Neon edge glow",
        hint: "Saturated contours with restrained bloom",
    },
    EffectInfo {
        id: "art_pixel_sort",
        label: "Pixel-sort trails",
        hint: "Bright pixel runs sorted into data trails",
    },
    EffectInfo {
        id: "art_chromatic_echo",
        label: "Chromatic echo",
        hint: "Offset magenta / cyan double-exposure edges",
    },
];

const REALISTIC_PRESETS: &[(&str, &str, &str, &[(&str, f32)])] = &[
    (
        "subtle",
        "Subtle",
        "A light transmission imperfection",
        &[
            ("jpeg_databend", 0.25),
            ("slice_shift", 0.25),
            ("streak_lines", 0.25),
            ("channel_shift", 0.22),
            ("edge_corruption", 0.25),
            ("analog_noise", 0.2),
        ],
    ),
    (
        "edge",
        "Edge",
        "Torn edges, displaced blocks and signal fragments",
        &[
            ("block_displace", 0.3),
            ("slice_shift", 0.35),
            ("bit_flip", 0.25),
            ("macroblock_decay", 0.3),
            ("streak_lines", 0.65),
            ("edge_corruption", 1.0),
        ],
    ),
    (
        "jpeg",
        "JPEG",
        "Compression breakup and chroma artefacts",
        &[
            ("jpeg_databend", 1.0),
            ("chroma_bleed", 0.75),
            ("macroblock_decay", 0.55),
            ("color_banding", 0.4),
            ("streak_lines", 0.25),
        ],
    ),
    (
        "buffer",
        "Buffer",
        "Stalls, smears, ghosting and decoder dropouts",
        &[
            ("stall_smear", 1.0),
            ("ghost", 0.75),
            ("slice_shift", 0.7),
            ("dropout_blocks", 0.55),
            ("interlace_artifact", 0.3),
            ("streak_lines", 0.45),
        ],
    ),
    (
        "stride",
        "Stride",
        "Memory stride drift and channel misalignment",
        &[
            ("stride_drift", 0.9),
            ("channel_shift", 0.65),
            ("slice_shift", 0.55),
            ("bit_flip", 0.35),
            ("rolling_shutter", 0.4),
        ],
    ),
    (
        "datamosh",
        "Datamosh",
        "Aggressive motion-block and decoder corruption",
        &[
            ("block_displace", 1.2),
            ("stall_smear", 0.7),
            ("macroblock_decay", 0.9),
            ("chroma_bleed", 0.65),
            ("dropout_blocks", 0.45),
            ("ghost", 0.3),
        ],
    ),
    (
        "analog",
        "Analog",
        "Sensor noise, scanlines and interlace texture",
        &[
            ("analog_noise", 0.8),
            ("scanline_noise", 0.8),
            ("interlace_artifact", 0.4),
            ("chromatic_aberration", 0.35),
            ("streak_lines", 0.25),
        ],
    ),
    (
        "chromatic",
        "Chromatic",
        "Colour-plane offsets and chroma distortion",
        &[
            ("chromatic_aberration", 0.9),
            ("channel_shift", 0.6),
            ("chroma_bleed", 0.55),
            ("color_banding", 0.3),
        ],
    ),
    (
        "heavy",
        "Heavy",
        "A dense combination of transmission and memory failures",
        &[
            ("stride_drift", 0.8),
            ("block_displace", 1.1),
            ("slice_shift", 0.9),
            ("stall_smear", 0.9),
            ("jpeg_databend", 1.0),
            ("chroma_bleed", 0.85),
            ("macroblock_decay", 0.9),
            ("bit_flip", 0.75),
            ("channel_shift", 0.8),
            ("dropout_blocks", 0.8),
            ("ghost", 0.55),
            ("streak_lines", 0.85),
            ("edge_corruption", 1.2),
            ("analog_noise", 0.35),
            ("color_banding", 0.4),
            ("interlace_artifact", 0.35),
        ],
    ),
];

const ARTISTIC_PRESETS: &[(&str, &str, &str, &[(&str, f32)])] = &[
    (
        "crimson_dream",
        "Crimson Dream",
        "A cinematic crimson signal treatment",
        &[
            ("art_duotone", 1.0),
            ("art_wave_warp", 0.85),
            ("art_scanlines", 0.85),
            ("art_halftone", 0.22),
        ],
    ),
    (
        "signal_burst",
        "Signal Burst",
        "Bright streaks, neon contours and colour echoes",
        &[
            ("art_light_streaks", 1.15),
            ("art_neon_edges", 0.72),
            ("art_chromatic_echo", 0.58),
            ("art_pixel_sort", 0.38),
        ],
    ),
    (
        "analog_vision",
        "Analog Vision",
        "Warped analog texture with a graphic print finish",
        &[
            ("art_duotone", 0.78),
            ("art_wave_warp", 1.15),
            ("art_scanlines", 1.0),
            ("art_halftone", 0.34),
            ("art_chromatic_echo", 0.25),
        ],
    ),
    (
        "glitch_cinema",
        "Glitch Cinema",
        "Fragmented luminous trails and neon cinema edges",
        &[
            ("art_light_streaks", 0.95),
            ("art_pixel_sort", 0.78),
            ("art_wave_warp", 0.56),
            ("art_neon_edges", 0.64),
            ("art_chromatic_echo", 0.48),
        ],
    ),
];

pub fn realistic_effects() -> &'static [EffectInfo] {
    &REALISTIC_EFFECTS
}
pub fn artistic_effects() -> &'static [EffectInfo] {
    &ARTISTIC_EFFECTS
}

pub fn realistic_presets() -> Vec<PresetInfo> {
    REALISTIC_PRESETS
        .iter()
        .map(|(id, label, description, _)| PresetInfo {
            id,
            label,
            description,
        })
        .collect()
}

pub fn artistic_presets() -> Vec<PresetInfo> {
    ARTISTIC_PRESETS
        .iter()
        .map(|(id, label, description, _)| PresetInfo {
            id,
            label,
            description,
        })
        .collect()
}

pub fn realistic_preset(id: &str) -> BTreeMap<String, f32> {
    REALISTIC_PRESETS
        .iter()
        .find(|(key, _, _, _)| *key == id)
        .map(|(_, _, _, values)| {
            values
                .iter()
                .map(|(name, value)| ((*name).to_string(), *value))
                .collect()
        })
        .unwrap_or_default()
}

pub fn artistic_preset(id: &str) -> BTreeMap<String, f32> {
    ARTISTIC_PRESETS
        .iter()
        .find(|(key, _, _, _)| *key == id)
        .map(|(_, _, _, values)| {
            values
                .iter()
                .map(|(name, value)| ((*name).to_string(), *value))
                .collect()
        })
        .unwrap_or_default()
}

pub fn all_realistic_preset_ids() -> Vec<&'static str> {
    REALISTIC_PRESETS.iter().map(|(id, _, _, _)| *id).collect()
}

pub fn all_artistic_preset_ids() -> Vec<&'static str> {
    ARTISTIC_PRESETS.iter().map(|(id, _, _, _)| *id).collect()
}

pub fn labels() -> Vec<(&'static str, &'static str)> {
    REALISTIC_EFFECTS
        .iter()
        .chain(ARTISTIC_EFFECTS.iter())
        .map(|e| (e.id, e.label))
        .collect()
}
