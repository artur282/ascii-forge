//! Machine-readable capability catalog for agents.

use anyhow::Result;
use serde_json::{Value, json};

use super::print_json;

/// Commands available in every build.
const COMMANDS: &[(&str, &str)] = &[
    (
        "generate",
        "Generate a scene from a prompt or an offline template.",
    ),
    ("make", "Generate and render a static image in one step."),
    ("render", "Render a scene to an image, text, GIF or video."),
    ("preview", "Preview a scene as animated ANSI art."),
    ("import", "Import an image or video/GIF as an ASCII scene."),
    ("cache", "Clear the project-local response cache."),
    (
        "fonts",
        "List the FIGlet fonts and the resolved render font.",
    ),
    (
        "doctor",
        "Check FFmpeg, encoders, fonts and the config directory.",
    ),
    ("capabilities", "Print this catalog."),
];

/// Template name as accepted by `--template`, plus its fixed aspect and layout.
const TEMPLATES: &[(&str, &str, &str, &str)] = &[
    (
        "logo",
        "landscape",
        "centered",
        "Bold title with a typewriter reveal.",
    ),
    (
        "promo",
        "landscape",
        "stacked",
        "Announcement with a slide-in title.",
    ),
    (
        "glitch",
        "landscape",
        "full_bleed",
        "Glitch and scanline energy.",
    ),
    (
        "matrix",
        "portrait",
        "full_bleed",
        "Green-on-black dissolve.",
    ),
    (
        "lower-third",
        "landscape",
        "lower_third",
        "Caption bar for video overlays.",
    ),
];

const STATIC_FORMATS: &[&str] = &["png", "jpeg", "svg", "ansi", "text"];
const ANIMATION_FORMATS: &[&str] = &["gif"];
const VIDEO_FORMATS: &[&str] = &["mp4", "webm"];
const CHARSETS: &[&str] = &["ascii", "blocks", "detailed"];
const EFFECTS: &[&str] = &[
    "typewriter",
    "fade",
    "slide",
    "scanline",
    "glitch",
    "wave",
    "dissolve",
    "blink",
];

const EXAMPLES: &[&str] = &[
    "ascii-forge make \"Shipping v1\" --template promo --out banner.png --json",
    "ascii-forge capabilities --json",
    "ascii-forge generate --template matrix --title \"ENTER\" --out scene.json",
    "ascii-forge render scene.json --format gif --out loop.gif",
    "ascii-forge import logo.png --edges --color --out scene.json",
];

pub fn run(json_output: bool) -> Result<()> {
    if json_output {
        print_json(&payload())
    } else {
        print_human();
        Ok(())
    }
}

fn payload() -> Value {
    json!({
        "name": "ascii-forge",
        "version": env!("CARGO_PKG_VERSION"),
        "commands": commands(),
        "formats": {
            "static": STATIC_FORMATS,
            "animation": ANIMATION_FORMATS,
            "video": VIDEO_FORMATS,
        },
        "templates": templates(),
        "charsets": CHARSETS,
        "effects": EFFECTS,
        "examples": EXAMPLES,
    })
}

fn commands() -> Vec<Value> {
    let mut commands: Vec<Value> = COMMANDS
        .iter()
        .map(|(name, summary)| json!({ "name": name, "summary": summary }))
        .collect();
    if cfg!(feature = "mcp") {
        commands.push(json!({
            "name": "mcp",
            "summary": "Start the Model Context Protocol server over stdio.",
        }));
    }
    commands
}

fn templates() -> Vec<Value> {
    TEMPLATES
        .iter()
        .map(|(name, aspect, layout, description)| {
            json!({
                "name": name,
                "aspect": aspect,
                "layout": layout,
                "description": description,
                "required": ["title"],
                "optional": ["tagline", "seed"],
            })
        })
        .collect()
}

fn command_names() -> Vec<&'static str> {
    let mut names: Vec<&'static str> = COMMANDS.iter().map(|(name, _)| *name).collect();
    if cfg!(feature = "mcp") {
        names.push("mcp");
    }
    names
}

fn print_human() {
    let names = command_names();
    let templates: Vec<&str> = TEMPLATES.iter().map(|(name, ..)| *name).collect();

    println!("ascii-forge {}", env!("CARGO_PKG_VERSION"));
    println!("commands: {}", names.join(", "));
    println!("templates: {}", templates.join(", "));
    println!(
        "formats: static={} animation={} video={}",
        STATIC_FORMATS.join(","),
        ANIMATION_FORMATS.join(","),
        VIDEO_FORMATS.join(",")
    );
    println!("run `ascii-forge capabilities --json` for the full contract");
}
