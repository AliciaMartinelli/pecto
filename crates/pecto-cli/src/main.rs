use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use owo_colors::OwoColorize;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "pecto")]
#[command(about = "Extract behavior specs from code through static analysis")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Analyze a project and generate behavior specs
    Init {
        /// Path to the project directory
        #[arg(default_value = ".")]
        path: PathBuf,

        /// Output format
        #[arg(short, long, default_value = "yaml")]
        format: String,

        /// Write specs to file instead of stdout
        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    /// Show the spec for a specific capability
    Show {
        /// Capability name (e.g., "user-authentication")
        name: String,

        /// Path to the project directory
        #[arg(short, long, default_value = ".")]
        path: PathBuf,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init {
            path,
            format,
            output,
        } => cmd_init(&path, &format, output.as_deref()),
        Commands::Show { name, path } => cmd_show(&name, &path),
    }
}

fn cmd_init(path: &PathBuf, format: &str, output: Option<&std::path::Path>) -> Result<()> {
    let abs_path = std::fs::canonicalize(path)
        .with_context(|| format!("Cannot find directory: {}", path.display()))?;

    eprintln!(
        "{} Analyzing {}...",
        "pecto".bold().cyan(),
        abs_path.display()
    );

    let spec = pecto_java::analyze_project(&abs_path).with_context(|| "Analysis failed")?;

    let total_endpoints: usize = spec.capabilities.iter().map(|c| c.endpoints.len()).sum();

    eprintln!(
        "{} Analyzed {} files → {} capabilities, {} endpoints\n",
        "✓".bold().green(),
        spec.files_analyzed,
        spec.capabilities.len().to_string().bold(),
        total_endpoints.to_string().bold(),
    );

    let output_str = match format {
        "json" => pecto_core::output::to_json(&spec).context("Failed to serialize to JSON")?,
        _ => pecto_core::output::to_yaml(&spec).context("Failed to serialize to YAML")?,
    };

    match output {
        Some(out_path) => {
            std::fs::write(out_path, &output_str)
                .with_context(|| format!("Failed to write to {}", out_path.display()))?;
            eprintln!(
                "{} Spec written to {}",
                "✓".bold().green(),
                out_path.display()
            );
        }
        None => {
            println!("{output_str}");
        }
    }

    Ok(())
}

fn cmd_show(name: &str, path: &PathBuf) -> Result<()> {
    let abs_path = std::fs::canonicalize(path)
        .with_context(|| format!("Cannot find directory: {}", path.display()))?;

    let spec = pecto_java::analyze_project(&abs_path).with_context(|| "Analysis failed")?;

    let capability = spec
        .capabilities
        .iter()
        .find(|c| c.name == name || c.name.contains(name));

    match capability {
        Some(cap) => {
            let yaml = serde_yaml::to_string(cap).context("Failed to serialize")?;
            println!("{yaml}");
        }
        None => {
            eprintln!(
                "{} Capability '{}' not found. Available:",
                "✗".bold().red(),
                name
            );
            for cap in &spec.capabilities {
                eprintln!("  - {}", cap.name.bold());
            }
        }
    }

    Ok(())
}
