use indexmap::IndexMap;
use serde_derive::{Deserialize, Serialize};
use std::ffi::OsStr;
use std::fs;
use std::path::Path;

#[derive(Clone, Serialize, Deserialize)]
pub struct Animation {
    #[serde(rename = "type")]
    pub _type: Option<String>,
    pub frames: Vec<String>,
    pub settings: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Define {
    pub name: String,
    pub value: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct KllHeader {
    pub name: String,
    pub variant: Option<String>,
    pub layout: String,
    pub base: String,
    pub version: String,
    pub author: String,
    #[serde(rename = "KLL")]
    pub kll: String,
    pub date: String,
    pub generator: String,
    #[serde(flatten)]
    pub other: serde_json::Map<String, serde_json::Value>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Led {
    pub id: usize,
    #[serde(rename = "scanCode")]
    pub scan_code: Option<String>,
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct KeyAction {
    pub key: String,
    pub label: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Trigger {
    pub action: String,
    pub label: String,
    #[serde(rename = "type")]
    pub _type: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct MatrixKey {
    pub code: String,
    pub x: Option<f32>,
    pub y: Option<f32>,
    pub w: Option<f32>,
    pub h: Option<f32>,
    pub layers: IndexMap<usize, KeyAction>,
    pub triggers: Option<IndexMap<usize, Trigger>>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct AnimationSetting {
    pub name: String,
    #[serde(rename = "type")]
    pub _type: String,
    pub default: serde_json::Value,
    pub values: Option<Vec<serde_json::Value>>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct CannedAnimation {
    pub settings: String,
    #[serde(rename = "type")]
    pub _type: String,
    pub description: String,
    pub configurable: Vec<AnimationSetting>,
    pub frames: Vec<String>,
    #[serde(rename = "custom-kll")]
    pub custom_kll: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct KllConfig {
    pub matrix: Vec<MatrixKey>,
    pub custom: Option<IndexMap<usize, String>>,
    pub animations: Option<IndexMap<String, Animation>>,
    pub canned: Option<IndexMap<String, CannedAnimation>>,
    pub defines: Option<Vec<Define>>,
    pub header: KllHeader,
    pub leds: Option<Vec<Led>>,
}

pub struct KllFile {
    pub content: String,
    pub name: String,
}

fn layout_matrix(filename: &str) -> Vec<MatrixKey> {
    println!("Reading {}", filename);
    let json: KllConfig = {
        let contents = fs::read_to_string(filename).expect("Missing layout");
        serde_json::from_str(&contents).unwrap()
    };
    json.matrix
}

fn crop_str(s: &str, pos: usize) -> &str {
    match s.char_indices().skip(pos).next() {
        Some((pos, _)) => &s[pos..],
        None => "",
    }
}

pub fn kll_filename(filename: &str) -> &str {
    let basename = Path::new(filename).file_stem().unwrap_or(OsStr::new(""));
    basename.to_str().unwrap_or("")
}

pub fn kll_layer(filenames: Vec<String>) -> String {
    filenames
        .iter()
        .map(|f| kll_filename(f))
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn kll_list(layers: Vec<String>) -> String {
    layers.join(";")
}

pub fn generate_kll(config: &KllConfig, is_lts: bool) -> Vec<KllFile> {
    let header = config.header.clone();
    let name = &header.name.replace(" ", "_"); //sanitize
    let variant = header.variant.unwrap_or("".to_string()).replace(" ", "_");
    let layout = header.layout.clone();
    let base_layout = &header.base;

    let mut files = Vec::new();
    if name.is_empty() || layout.is_empty() {
        // Invalid Header Information
        return files;
    }

    let mut default = layout_matrix(&format!("./layouts/{}-{}.json", name, base_layout));

    let mut layers: Vec<Vec<(String, String)>> = Vec::new();
    let triggers: Vec<Vec<(String, Vec<Trigger>)>> = Vec::new();

    // Find the differences between the default map and the user's map
    match name.to_lowercase().as_ref() {
        // WhiteFox layouts have fewer keys than the defaultMap so we need to verify based
        //  upon the scan codes rather than just a sequence. Long term this method should
        //  probably be the preferred method for building up layer files
        "whitefox" => {
            if is_lts {
                // Between LTS and Latest the scancode mapping for White Fox changed. Previously
                //  there was a single all encompassing map, now there are a number of smaller
                //  ones that have different (sensible) default scancode mappings. This causes
                //  a little bit of havok due to the way layering works, we override what was
                //  previously there, we'll look for a special `.lts.json` file here.
                let lts_base_name = format!("./layouts/{}-{}.lts.json", name, base_layout);
                default = layout_matrix(&lts_base_name);
            }

            for (_i, key) in config.matrix.iter().enumerate() {
                // First find the corresponding key via scan code
                let idx_in_def = default.iter().position(|def_key| key.code == def_key.code);

                if let Some(idx_in_def) = idx_in_def {
                    for (l, layer) in key.layers.iter() {
                        let l = *l;
                        if !layers.get(l).is_some() {
                            layers.resize(l + 1, Vec::new());
                        }
                        layers[l].push((
                            default[idx_in_def].layers.get(&0).unwrap().key.clone(),
                            layer.key.clone(),
                        ));
                    }

                    // Process "trigger" entries
                    if !is_lts {
                        if let Some(ts) = &key.triggers {
                            for (_t, _trigger) in ts {
                                // TODO
                                //triggers[t][&default[idx_in_def].layers.get(&0).unwrap().key].push(trigger);
                            }
                        }
                    }
                }
            }
        }
        _ => {
            for (i, key) in config.matrix.iter().enumerate() {
                // TODO: Dedup with ergodox
                // Process "layer" entries
                for (l, layer) in key.layers.iter() {
                    let l = *l;
                    if !layers.get(l).is_some() {
                        layers.resize(l + 1, Vec::new());
                    }
                    layers[l].push((
                        default[i].layers.get(&0).unwrap().key.clone(),
                        layer.key.clone(),
                    ));
                }

                // Process "trigger" entries
                if !is_lts {
                    if let Some(ts) = &key.triggers {
                        for (_t, _trigger) in ts {
                            // TODO
                            //triggers[t][&default[i].layers.get(&0).unwrap().key].push(trigger);
                        }
                    }
                }
            }
        }
    }

    let mut headers: IndexMap<String, String> = IndexMap::new();
    headers.insert("Name".to_string(), header.name);
    headers.insert("Variant".to_string(), variant);
    headers.insert("Layout".to_string(), header.layout);
    headers.insert("Base".to_string(), header.base);
    headers.insert("Version".to_string(), header.version);
    headers.insert("Author".to_string(), header.author);
    headers.insert("KLL".to_string(), header.kll);
    headers.insert("Date".to_string(), header.date);
    headers.insert("Generator".to_string(), header.generator);

    let header = headers
        .iter()
        .map(|(k, v)| format!("{} = \"{}\";", k, v))
        .collect::<Vec<_>>()
        .join("\n");
    let defines = match &config.defines {
        Some(d) => d
            .iter()
            .map(|define| format!("{} = \"{}\";", define.name, define.value))
            .collect::<Vec<_>>()
            .join("\n"),
        None => "".to_string(),
    };

    //let mut file_args = Vec::new();
    let _controller_ver =
        "4f4e8e0def57585a887a89b07f79b4b9889eb15af42eca6a53c84bcd093a0e12149789db4f14f98c"; //controller_rev . kll_rev
                                                                                            // let hashbaby = "";
    let layout_name = format!("{}-{}", name, layout);

    let mut animations = "".to_string();
    let mut ignored_animations = Vec::new();
    if !is_lts {
        match &config.animations {
            Some(a) => {
                animations = a
                    .iter()
                    .map(|(k, v)| {
                        let mut s = format!("A[{}] <= {};\n", k, v.settings);

                        let mut i = 1; // TODO: Use enumerate here
                        for frame in v.frames.iter() {
                            if frame.starts_with("#") {
                                s.push_str(&format!("{}\n", frame));
                            } else {
                                s.push_str(&format!("A[{}, {}] <= {};\n", k, i, frame));
                                i += 1;
                            }
                        }
                        if i > 1 {
                            return s;
                        } else {
                            ignored_animations.push(k);
                            return format!("### {} is empty, skipping", k);
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
            }
            _ => {}
        }
    }

    // Generate .kll files
    for (n, layer) in layers.iter().enumerate() {
        let out = layer
            .iter()
            .map(|(k, v)| {
                let mut comment_out = false;
                let mut s = v.to_string();
                if v.starts_with("#:") {
                    if is_lts && v.contains("ledControl") {
                        let m = v.replace(" ", "");
                        if m.contains("ledControl(0,15)") {
                            // LED-
                            s = "ledControl( 3, 15, 0 )".to_string();
                        } else if m.contains("ledControl(1,15)") {
                            // LED+
                            s = "ledControl( 4, 15, 0 )".to_string();
                        } else if m.contains("ledControl(3,0)") {
                            // LED OFF
                            s = "ledControl( 5, 0, 0)".to_string();
                        } else {
                            comment_out = true;
                        }
                    } else if is_lts && v.contains("animation_control") {
                        comment_out = true;
                    } else {
                        s = crop_str(v, 2).to_string();
                    }
                } else if v.starts_with("CONS:") {
                    s = format!("CONS\"{}\"", crop_str(v, 5));
                } else if v.starts_with("SYS:") {
                    s = format!("SYS\"{}\"", crop_str(v, 4));
                } else {
                    s = format!("U\"{}\"", v);
                }

                if comment_out {
                    format!("#U\"{}\" : {};", k, s)
                } else {
                    format!("U\"{}\" : {};", k, s)
                }
            })
            .collect::<Vec<_>>()
            .join("\n");

        let mut triggers_out = "".to_string();
        if let Some(triggers) = triggers.get(n) {
            triggers_out = triggers
                .iter()
                .map(|(k, v)| {
                    v.iter()
                        .map(|t| format!("U\"{}\" :+ {};", k, t.action))
                        .collect::<Vec<_>>()
                        .join("\n")
                })
                .collect::<Vec<_>>()
                .join("\n");
        }

        let custom = match &config.custom {
            Some(custom) => match custom.get(&n) {
                Some(c) => format!("\n\n{}", c),
                None => "".to_string(),
            },
            None => "".to_string(),
        };

        let out = {
            if n == 0 {
                format!(
                    "{}\n\n{}\n\n{}\n\n{}{}\n\n{}\n\n",
                    header, defines, out, triggers_out, custom, animations
                )
            } else {
                format!("{}\n\n{}\n\n{}{}\n\n", header, out, triggers_out, custom)
            }
        };

        files.push(KllFile {
            content: out,
            name: format!("{}-{}.kll", layout_name, n),
        });
    }

    return files;
}
