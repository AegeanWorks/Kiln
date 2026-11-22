//! Pure command-line argument parsing. No environment reads.

use crate::model::Format;

/// A parsed command; the shell decides what to do with it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    Generate {
        schema: String,
        format: Option<Format>,
        count: Option<usize>,
        out: Option<String>,
        edges: bool,
        seed: Option<u64>,
    },
    Check {
        schema: String,
        fixture: String,
        json: bool,
    },
    Diff {
        left: String,
        right: String,
        format: Option<Format>,
        json: bool,
    },
    Manifest {
        path: String,
        json: bool,
    },
    Corpus {
        schema: String,
        start: u64,
        end: u64,
        format: Option<Format>,
        out_dir: String,
    },
    Fields {
        schema: String,
        json: bool,
    },
    Export {
        schema: String,
    },
    Demo {
        seed: i64,
    },
    Inventory,
    Explain,
    Help,
}

/// Stable usage text, also printed by `--help`.
pub fn usage() -> String {
    let mut out = String::from("kiln — deterministic test-fixture forge\n\n");
    out.push_str("Usage:\n");
    out.push_str("  kiln generate <schema.kiln> [--format json|jsonl|csv] [--count N] [--out FILE] [--edges] [--seed N]\n");
    out.push_str("  a schema path of '-' reads the schema from stdin\n");
    out.push_str("  kiln check <schema.kiln> --fixture <FILE>\n");
    out.push_str("  kiln check --manifest kiln.toml\n");
    out.push_str("  kiln diff <left> <right> [--format json|jsonl|csv]\n");
    out.push_str("  kiln demo --scripted [--seed N]\n");
    out.push_str("  kiln corpus <schema.kiln> --seeds A..B --out-dir DIR [--format FMT]\n");
    out.push_str("  kiln fields <schema.kiln> [--json]\n");
    out.push_str("  kiln export <schema.kiln>          # draft-2020-12 JSON Schema\n");
    out.push_str("  kiln inventory\n");
    out.push_str("  kiln explain\n\n");
    out.push_str("Add --json to check, diff or check --manifest for a machine-readable report.\n");
    out.push_str("Exit codes: 0 success or match, 1 fixture mismatch, 2 misuse or I/O error.\n");
    out
}

fn error(message: &str) -> String {
    format!("{}\nWhy: a wrong command line cannot generate trustworthy fixtures.\nFix: kiln --help\n\n{}", message, usage())
}

fn take_value(args: &[String], index: &mut usize, flag: &str) -> Result<String, String> {
    *index += 1;
    args.get(*index)
        .cloned()
        .ok_or_else(|| error(&format!("{} needs a value", flag)))
}

/// Parse arguments into a command. All validation is pure string work.
pub fn parse(args: &[String]) -> Result<Command, String> {
    if args.is_empty() || args[0] == "--help" || args[0] == "-h" {
        return Ok(Command::Help);
    }
    let command = args[0].as_str();
    let rest = &args[1..];
    match command {
        "inventory" => Ok(Command::Inventory),
        "explain" => Ok(Command::Explain),
        "generate" => {
            let mut schema: Option<String> = None;
            let mut format = None;
            let mut count = None;
            let mut out = None;
            let mut edges = false;
            let mut seed = None;
            let mut i = 0;
            while i < rest.len() {
                match rest[i].as_str() {
                    "--seed" => {
                        let value = take_value(rest, &mut i, "--seed")?;
                        seed = Some(value.parse::<u64>().map_err(|_| error("--seed needs a non-negative integer"))?);
                    }
                    "--edges" => edges = true,
                    "--format" => {
                        let value = take_value(rest, &mut i, "--format")?;
                        format = Some(Format::parse(&value).map_err(|message| error(&message))?);
                    }
                    "--count" => {
                        let value = take_value(rest, &mut i, "--count")?;
                        count = Some(value.parse::<usize>().map_err(|_| error("--count needs a positive integer"))?);
                    }
                    "--out" => out = Some(take_value(rest, &mut i, "--out")?),
                    other if other.starts_with('-') && other != "-" => {
                        return Err(error(&format!("unknown flag '{}'", other)));
                    }
                    other => {
                        if schema.is_some() {
                            return Err(error("generate takes exactly one schema path"));
                        }
                        schema = Some(other.to_string());
                    }
                }
                i += 1;
            }
            let schema = schema.ok_or_else(|| error("generate needs a schema path"))?;
            Ok(Command::Generate { schema, format, count, out, edges, seed })
        }
        "check" if rest.first().map(|value| value.as_str()) == Some("--manifest") => {
            let mut path = None;
            let mut json = false;
            let mut i = 0;
            while i < rest.len() {
                match rest[i].as_str() {
                    "--manifest" => path = Some(take_value(rest, &mut i, "--manifest")?),
                    "--json" => json = true,
                    other if other.starts_with('-') => return Err(error(&format!("unknown flag '{}'", other))),
                    other => return Err(error(&format!("unexpected argument '{}' with --manifest", other))),
                }
                i += 1;
            }
            let path = path.ok_or_else(|| error("check --manifest needs a path"))?;
            Ok(Command::Manifest { path, json })
        }
        "diff" => {
            let mut paths = Vec::new();
            let mut format = None;
            let mut json = false;
            let mut i = 0;
            while i < rest.len() {
                match rest[i].as_str() {
                    "--json" => json = true,
                    "--format" => {
                        let value = take_value(rest, &mut i, "--format")?;
                        format = Some(Format::parse(&value).map_err(|message| error(&message))?);
                    }
                    other if other.starts_with('-') => return Err(error(&format!("unknown flag '{}'", other))),
                    other => paths.push(other.to_string()),
                }
                i += 1;
            }
            if paths.len() != 2 {
                return Err(error("diff needs exactly two fixture paths"));
            }
            Ok(Command::Diff {
                left: paths.remove(0),
                right: paths.remove(0),
                format,
                json,
            })
        }
        "check" => {
            let mut schema: Option<String> = None;
            let mut fixture = None;
            let mut json = false;
            let mut i = 0;
            while i < rest.len() {
                match rest[i].as_str() {
                    "--json" => json = true,
                    "--fixture" => fixture = Some(take_value(rest, &mut i, "--fixture")?),
                    other if other.starts_with('-') && other != "-" => {
                        return Err(error(&format!("unknown flag '{}'", other)));
                    }
                    other => {
                        if schema.is_some() {
                            return Err(error("check takes exactly one schema path"));
                        }
                        schema = Some(other.to_string());
                    }
                }
                i += 1;
            }
            let schema = schema.ok_or_else(|| error("check needs a schema path"))?;
            let fixture = fixture.ok_or_else(|| error("check needs --fixture <FILE>"))?;
            Ok(Command::Check { schema, fixture, json })
        }
        "export" => {
            if rest.len() != 1 || rest[0].starts_with('-') && rest[0] != "-" {
                return Err(error("export needs exactly one schema path"));
            }
            Ok(Command::Export { schema: rest[0].clone() })
        }
        "fields" => {
            let mut schema = None;
            let mut json = false;
            for item in rest {
                if item == "--json" {
                    json = true;
                } else if item.starts_with('-') && item != "-" {
                    return Err(error(&format!("unknown flag '{}'", item)));
                } else if schema.is_some() {
                    return Err(error("fields takes exactly one schema path"));
                } else {
                    schema = Some(item.clone());
                }
            }
            let schema = schema.ok_or_else(|| error("fields needs a schema path"))?;
            Ok(Command::Fields { schema, json })
        }
        "corpus" => {
            let mut schema = None;
            let mut seeds = None;
            let mut out_dir = None;
            let mut format = None;
            let mut i = 0;
            while i < rest.len() {
                match rest[i].as_str() {
                    "--seeds" => {
                        let value = take_value(rest, &mut i, "--seeds")?;
                        let parts: Vec<&str> = value.split("..").collect();
                        if parts.len() != 2 {
                            return Err(error("--seeds needs A..B"));
                        }
                        let start = parts[0].parse::<u64>().map_err(|_| error("--seeds needs non-negative integers"))?;
                        let end = parts[1].parse::<u64>().map_err(|_| error("--seeds needs non-negative integers"))?;
                        seeds = Some((start, end));
                    }
                    "--out-dir" => out_dir = Some(take_value(rest, &mut i, "--out-dir")?),
                    "--format" => {
                        let value = take_value(rest, &mut i, "--format")?;
                        format = Some(Format::parse(&value).map_err(|message| error(&message))?);
                    }
                    other if other.starts_with('-') && other != "-" => {
                        return Err(error(&format!("unknown flag '{}'", other)));
                    }
                    other => {
                        if schema.is_some() {
                            return Err(error("corpus takes exactly one schema path"));
                        }
                        schema = Some(other.to_string());
                    }
                }
                i += 1;
            }
            let schema = schema.ok_or_else(|| error("corpus needs a schema path"))?;
            let (start, end) = seeds.ok_or_else(|| error("corpus needs --seeds A..B"))?;
            let out_dir = out_dir.ok_or_else(|| error("corpus needs --out-dir DIR"))?;
            Ok(Command::Corpus { schema, start, end, format, out_dir })
        }
        "demo" => {
            if !rest.iter().any(|item| item == "--scripted") {
                return Err(error("demo needs --scripted"));
            }
            let mut seed = 0i64;
            let mut i = 0;
            while i < rest.len() {
                if rest[i] == "--seed" {
                    let value = take_value(rest, &mut i, "--seed")?;
                    seed = value.parse::<i64>().map_err(|_| error("--seed needs an integer"))?;
                }
                i += 1;
            }
            Ok(Command::Demo { seed })
        }
        other => Err(error(&format!("unknown command '{}'", other))),
    }
}
