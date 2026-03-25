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

    /// Verify that code matches an existing spec file
    Verify {
        /// Path to existing spec file
        spec: PathBuf,

        /// Path to the project directory
        #[arg(short, long, default_value = ".")]
        path: PathBuf,
    },

    /// Show behavior changes between two git refs
    Diff {
        /// First git ref (e.g., main, HEAD~1, a commit hash)
        base: String,

        /// Second git ref (defaults to current working tree)
        #[arg(default_value = "HEAD")]
        head: String,

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
        Commands::Verify { spec, path } => cmd_verify(&spec, &path),
        Commands::Diff { base, head, path } => cmd_diff(&base, &head, &path),
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

fn cmd_verify(spec_path: &PathBuf, path: &PathBuf) -> Result<()> {
    let abs_path = std::fs::canonicalize(path)
        .with_context(|| format!("Cannot find directory: {}", path.display()))?;

    // Read existing spec
    let spec_content = std::fs::read_to_string(spec_path)
        .with_context(|| format!("Cannot read spec file: {}", spec_path.display()))?;

    let format = if spec_path.extension().is_some_and(|e| e == "json") {
        "json"
    } else {
        "yaml"
    };

    eprintln!(
        "{} Verifying {} against {}...",
        "pecto".bold().cyan(),
        spec_path.display(),
        abs_path.display()
    );

    // Re-analyze the project
    let current_spec = pecto_java::analyze_project(&abs_path).with_context(|| "Analysis failed")?;

    let current_str = match format {
        "json" => pecto_core::output::to_json(&current_spec)
            .context("Failed to serialize current spec")?,
        _ => pecto_core::output::to_yaml(&current_spec)
            .context("Failed to serialize current spec")?,
    };

    // Compare
    if spec_content.trim() == current_str.trim() {
        eprintln!(
            "{} Spec matches code — no drift detected",
            "✓".bold().green()
        );
        return Ok(());
    }

    // Show diff
    eprintln!("{} Spec drift detected! Differences:\n", "✗".bold().red());

    use similar::{ChangeTag, TextDiff};
    let diff = TextDiff::from_lines(spec_content.trim(), current_str.trim());

    for change in diff.iter_all_changes() {
        match change.tag() {
            ChangeTag::Delete => eprint!("{}{}", "-".red(), change.to_string_lossy().red()),
            ChangeTag::Insert => eprint!("{}{}", "+".green(), change.to_string_lossy().green()),
            ChangeTag::Equal => eprint!(" {}", change.to_string_lossy().dimmed()),
        }
    }

    std::process::exit(1);
}

fn cmd_diff(base: &str, head: &str, path: &PathBuf) -> Result<()> {
    let abs_path = std::fs::canonicalize(path)
        .with_context(|| format!("Cannot find directory: {}", path.display()))?;

    eprintln!("{} Comparing {} → {}...", "pecto".bold().cyan(), base, head);

    // Create temp directories for the two git refs
    let temp_dir = std::env::temp_dir().join("pecto-diff");
    let base_dir = temp_dir.join("base");
    let head_dir = temp_dir.join("head");

    // Clean up any previous run
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&base_dir).context("Failed to create temp dir")?;
    std::fs::create_dir_all(&head_dir).context("Failed to create temp dir")?;

    // Export git refs to temp directories
    export_git_ref(&abs_path, base, &base_dir)?;
    export_git_ref(&abs_path, head, &head_dir)?;

    // Analyze both
    let base_spec =
        pecto_java::analyze_project(&base_dir).with_context(|| "Failed to analyze base ref")?;
    let head_spec =
        pecto_java::analyze_project(&head_dir).with_context(|| "Failed to analyze head ref")?;

    let base_yaml =
        pecto_core::output::to_yaml(&base_spec).context("Failed to serialize base spec")?;
    let head_yaml =
        pecto_core::output::to_yaml(&head_spec).context("Failed to serialize head spec")?;

    // Clean up
    let _ = std::fs::remove_dir_all(&temp_dir);

    if base_yaml == head_yaml {
        eprintln!(
            "{} No behavior changes between {} and {}",
            "✓".bold().green(),
            base,
            head
        );
        return Ok(());
    }

    eprintln!(
        "{} Behavior changes between {} and {}:\n",
        "!".bold().yellow(),
        base,
        head
    );

    use similar::{ChangeTag, TextDiff};
    let diff = TextDiff::from_lines(&base_yaml, &head_yaml);

    for change in diff.iter_all_changes() {
        match change.tag() {
            ChangeTag::Delete => eprint!("{}{}", "-".red(), change.to_string_lossy().red()),
            ChangeTag::Insert => eprint!("{}{}", "+".green(), change.to_string_lossy().green()),
            ChangeTag::Equal => {}
        }
    }

    Ok(())
}

/// Export files from a git ref to a target directory using git archive.
fn export_git_ref(
    repo_path: &std::path::Path,
    git_ref: &str,
    target: &std::path::Path,
) -> Result<()> {
    let output = std::process::Command::new("git")
        .args(["archive", "--format=tar", git_ref])
        .current_dir(repo_path)
        .stdout(std::process::Stdio::piped())
        .output()
        .context("Failed to run git archive")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git archive failed for ref '{}': {}", git_ref, stderr);
    }

    let output2 = std::process::Command::new("tar")
        .args(["xf", "-"])
        .current_dir(target)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .context("Failed to run tar")?;

    use std::io::Write;
    let mut child = output2;
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(&output.stdout)
        .context("Failed to pipe to tar")?;

    let tar_result = child.wait_with_output().context("tar failed")?;
    if !tar_result.status.success() {
        anyhow::bail!("tar extraction failed");
    }

    Ok(())
}
