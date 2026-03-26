mod dashboard;
mod report;
mod serve;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use owo_colors::OwoColorize;
use pecto_core::model::ProjectSpec;
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(name = "pecto")]
#[command(about = "Extract behavior specs from code through static analysis")]
#[command(version)]
struct Cli {
    /// Language to analyze (auto-detected if not specified)
    #[arg(short, long, global = true, default_value = "auto")]
    language: Language,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Clone, ValueEnum)]
enum Language {
    Auto,
    Java,
    Csharp,
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

        /// Show detailed per-capability breakdown
        #[arg(short, long)]
        verbose: bool,

        /// Suppress all status output (only print spec)
        #[arg(short, long)]
        quiet: bool,
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

    /// Show domain clusters (grouped capabilities)
    Domains {
        /// Path to the project directory
        #[arg(default_value = ".")]
        path: PathBuf,
    },

    /// Show dependency graph
    Graph {
        /// Path to the project directory
        #[arg(default_value = ".")]
        path: PathBuf,

        /// Output format: text, dot, json
        #[arg(short, long, default_value = "text")]
        format: String,
    },

    /// Export compact AI/LLM-readable context
    Context {
        /// Path to the project directory
        #[arg(default_value = ".")]
        path: PathBuf,
    },

    /// Generate a self-contained HTML report with dependency graph
    Report {
        /// Path to the project directory
        #[arg(default_value = ".")]
        path: PathBuf,

        /// Output HTML file
        #[arg(short, long, default_value = "pecto-report.html")]
        output: PathBuf,
    },

    /// Show impact of changing a capability
    Impact {
        /// Capability name to analyze
        name: String,

        /// Path to the project directory
        #[arg(default_value = ".")]
        path: PathBuf,
    },

    /// Start an interactive web dashboard
    Serve {
        /// Path to the project directory
        #[arg(default_value = ".")]
        path: PathBuf,

        /// Port to listen on
        #[arg(long, default_value = "4321")]
        port: u16,
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
            verbose,
            quiet,
        } => cmd_init(
            &path,
            &format,
            output.as_deref(),
            verbose,
            quiet,
            &cli.language,
        ),
        Commands::Show { name, path } => cmd_show(&name, &path, &cli.language),
        Commands::Domains { path } => cmd_domains(&path, &cli.language),
        Commands::Context { path } => cmd_context(&path, &cli.language),
        Commands::Graph { path, format } => cmd_graph(&path, &format, &cli.language),
        Commands::Report { path, output } => cmd_report(&path, &output, &cli.language),
        Commands::Impact { name, path } => cmd_impact(&name, &path, &cli.language),
        Commands::Serve { path, port } => cmd_serve(&path, port, &cli.language),
        Commands::Verify { spec, path } => cmd_verify(&spec, &path, &cli.language),
        Commands::Diff { base, head, path } => cmd_diff(&base, &head, &path, &cli.language),
    }
}

/// Detect project language from files in the directory.
fn detect_language(path: &Path) -> Result<Language> {
    // Check for project files
    for entry in walkdir::WalkDir::new(path)
        .max_depth(3)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let name = entry.file_name().to_string_lossy();
        if name.ends_with(".csproj") || name.ends_with(".sln") {
            return Ok(Language::Csharp);
        }
        if name == "pom.xml" || name == "build.gradle" || name == "build.gradle.kts" {
            return Ok(Language::Java);
        }
    }

    // Fallback: count file extensions
    let mut java_count = 0usize;
    let mut cs_count = 0usize;
    for entry in walkdir::WalkDir::new(path)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if let Some(ext) = entry.path().extension() {
            if ext == "java" {
                java_count += 1;
            } else if ext == "cs" {
                cs_count += 1;
            }
        }
    }

    if cs_count > java_count && cs_count > 0 {
        Ok(Language::Csharp)
    } else if java_count > 0 {
        Ok(Language::Java)
    } else {
        anyhow::bail!("Could not detect project language. Use --language java or --language csharp")
    }
}

/// Analyze a project using the appropriate language analyzer.
fn analyze(path: &Path, language: &Language) -> Result<ProjectSpec> {
    let abs_path =
        std::fs::canonicalize(path).with_context(|| format!("Cannot find: {}", path.display()))?;

    let lang = match language {
        Language::Auto => detect_language(&abs_path)?,
        other => other.clone(),
    };

    let mut spec = match lang {
        Language::Java => pecto_java::analyze_project(&abs_path)
            .map_err(|e| anyhow::anyhow!("{}", e))
            .context("Java analysis failed")?,
        Language::Csharp => pecto_csharp::analyze_project(&abs_path)
            .map_err(|e| anyhow::anyhow!("{}", e))
            .context("C# analysis failed")?,
        Language::Auto => unreachable!(),
    };

    // Post-processing: cluster capabilities into domains
    pecto_core::domains::cluster_domains(&mut spec);

    // Sort capabilities for stable output across platforms
    spec.capabilities.sort_by(|a, b| a.name.cmp(&b.name));

    Ok(spec)
}

fn cmd_init(
    path: &Path,
    format: &str,
    output: Option<&Path>,
    verbose: bool,
    quiet: bool,
    language: &Language,
) -> Result<()> {
    if !quiet {
        eprintln!("{} Analyzing {}...", "pecto".bold().cyan(), path.display());
    }

    let spec = analyze(path, language)?;

    if !quiet {
        let total_endpoints: usize = spec.capabilities.iter().map(|c| c.endpoints.len()).sum();
        let total_entities: usize = spec.capabilities.iter().map(|c| c.entities.len()).sum();
        let total_operations: usize = spec.capabilities.iter().map(|c| c.operations.len()).sum();
        let total_tasks: usize = spec
            .capabilities
            .iter()
            .map(|c| c.scheduled_tasks.len())
            .sum();

        eprintln!(
            "{} Analyzed {} files → {} capabilities\n",
            "✓".bold().green(),
            spec.files_analyzed,
            spec.capabilities.len().to_string().bold(),
        );

        // Summary table
        if total_endpoints > 0 {
            eprintln!("  {} endpoints", total_endpoints.to_string().bold());
        }
        if total_entities > 0 {
            eprintln!("  {} entities", total_entities.to_string().bold());
        }
        if total_operations > 0 {
            eprintln!("  {} operations", total_operations.to_string().bold());
        }
        if total_tasks > 0 {
            eprintln!("  {} scheduled tasks", total_tasks.to_string().bold());
        }

        if verbose {
            eprintln!();
            for cap in &spec.capabilities {
                let detail = if !cap.endpoints.is_empty() {
                    format!("{} endpoints", cap.endpoints.len())
                } else if !cap.entities.is_empty() {
                    format!("{} entities", cap.entities.len())
                } else if !cap.operations.is_empty() {
                    format!("{} operations", cap.operations.len())
                } else if !cap.scheduled_tasks.is_empty() {
                    format!("{} tasks", cap.scheduled_tasks.len())
                } else {
                    continue;
                };
                eprintln!("  {} {}", cap.name.bold(), detail.dimmed());
            }
        }

        eprintln!();
    }

    let output_str = match format {
        "json" => pecto_core::output::to_json(&spec).context("Failed to serialize to JSON")?,
        _ => pecto_core::output::to_yaml(&spec).context("Failed to serialize to YAML")?,
    };

    match output {
        Some(out_path) => {
            std::fs::write(out_path, &output_str)
                .with_context(|| format!("Failed to write to {}", out_path.display()))?;
            if !quiet {
                eprintln!(
                    "{} Spec written to {}",
                    "✓".bold().green(),
                    out_path.display()
                );
            }
        }
        None => {
            println!("{output_str}");
        }
    }

    Ok(())
}

fn cmd_show(name: &str, path: &Path, language: &Language) -> Result<()> {
    let spec = analyze(path, language)?;

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

fn cmd_verify(spec_path: &Path, path: &Path, language: &Language) -> Result<()> {
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
        path.display()
    );

    let mut current_spec = analyze(path, language)?;

    // Parse the stored spec's timestamp and use it for comparison
    // so that the `analyzed` field doesn't cause false drift detection
    let stored_timestamp = if format == "json" {
        serde_json::from_str::<pecto_core::model::ProjectSpec>(&spec_content)
            .ok()
            .and_then(|s| s.analyzed)
    } else {
        serde_yaml::from_str::<pecto_core::model::ProjectSpec>(&spec_content)
            .ok()
            .and_then(|s| s.analyzed)
    };
    if let Some(ts) = stored_timestamp {
        current_spec.analyzed = Some(ts);
    }

    // Sort capabilities by name for stable comparison (file traversal order varies by platform)
    current_spec
        .capabilities
        .sort_by(|a, b| a.name.cmp(&b.name));

    let current_str = match format {
        "json" => pecto_core::output::to_json(&current_spec)
            .context("Failed to serialize current spec")?,
        _ => pecto_core::output::to_yaml(&current_spec)
            .context("Failed to serialize current spec")?,
    };

    if spec_content.trim() == current_str.trim() {
        eprintln!(
            "{} Spec matches code — no drift detected",
            "✓".bold().green()
        );
        return Ok(());
    }

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

fn cmd_serve(path: &Path, port: u16, language: &Language) -> Result<()> {
    let spec = analyze(path, language)?;
    eprintln!("{} Starting pecto dashboard...\n", "pecto".bold().cyan(),);
    serve::serve(spec, port)
}

fn cmd_report(path: &Path, output: &Path, language: &Language) -> Result<()> {
    let spec = analyze(path, language)?;
    report::generate_report(&spec, output)?;
    eprintln!(
        "{} Report written to {}",
        "✓".bold().green(),
        output.display()
    );
    Ok(())
}

fn cmd_context(path: &Path, language: &Language) -> Result<()> {
    let spec = analyze(path, language)?;
    let ctx = pecto_core::context_export::to_context(&spec);
    println!("{ctx}");
    Ok(())
}

fn cmd_domains(path: &Path, language: &Language) -> Result<()> {
    let spec = analyze(path, language)?;

    if spec.domains.is_empty() {
        eprintln!("{} No domains found", "!".yellow());
        return Ok(());
    }

    for domain in &spec.domains {
        println!(
            "{} ({})",
            domain.name.bold(),
            format!("{} capabilities", domain.capabilities.len()).dimmed()
        );
        for cap in &domain.capabilities {
            println!("  - {}", cap);
        }
        if !domain.external_dependencies.is_empty() {
            println!(
                "  {} {}",
                "depends on:".dimmed(),
                domain.external_dependencies.join(", ").cyan()
            );
        }
        println!();
    }

    Ok(())
}

fn cmd_graph(path: &Path, format: &str, language: &Language) -> Result<()> {
    let spec = analyze(path, language)?;

    if spec.dependencies.is_empty() {
        eprintln!("{} No dependencies found", "!".yellow());
        return Ok(());
    }

    match format {
        "dot" => {
            println!("digraph pecto {{");
            println!("  rankdir=LR;");
            println!("  node [shape=box, style=rounded];");
            for dep in &spec.dependencies {
                println!(
                    "  \"{}\" -> \"{}\" [label=\"{:?}\"];",
                    dep.from, dep.to, dep.kind
                );
            }
            println!("}}");
        }
        "json" => {
            let json =
                serde_json::to_string_pretty(&spec.dependencies).context("Failed to serialize")?;
            println!("{json}");
        }
        _ => {
            // Text format
            for dep in &spec.dependencies {
                println!(
                    "{} {} {} {}",
                    dep.from.bold(),
                    "→".dimmed(),
                    dep.to.cyan(),
                    format!("({:?})", dep.kind).dimmed()
                );
                for reference in &dep.references {
                    println!("    {}", reference.dimmed());
                }
            }
        }
    }

    Ok(())
}

fn cmd_impact(name: &str, path: &Path, language: &Language) -> Result<()> {
    let spec = analyze(path, language)?;

    // Find all capabilities that match the name
    let matching: Vec<&str> = spec
        .capabilities
        .iter()
        .filter(|c| c.name.contains(name))
        .map(|c| c.name.as_str())
        .collect();

    if matching.is_empty() {
        eprintln!(
            "{} No capability matching '{}' found",
            "✗".bold().red(),
            name
        );
        return Ok(());
    }

    eprintln!("{} Impact analysis for '{}'\n", "pecto".bold().cyan(), name);

    // BFS: find all capabilities that depend on the matching ones (reverse traversal)
    let mut impacted: Vec<(String, Vec<String>)> = Vec::new();
    let mut visited = std::collections::HashSet::new();
    let mut queue: std::collections::VecDeque<(String, Vec<String>)> =
        std::collections::VecDeque::new();

    for m in &matching {
        queue.push_back((m.to_string(), vec![m.to_string()]));
        visited.insert(m.to_string());
    }

    while let Some((current, path_so_far)) = queue.pop_front() {
        // Find all capabilities that depend ON current (reverse edges)
        for dep in &spec.dependencies {
            if dep.to == current && !visited.contains(&dep.from) {
                visited.insert(dep.from.clone());
                let mut new_path = path_so_far.clone();
                new_path.push(dep.from.clone());
                queue.push_back((dep.from.clone(), new_path.clone()));
                impacted.push((dep.from.clone(), new_path));
            }
        }
    }

    if impacted.is_empty() {
        println!(
            "{} No other capabilities depend on '{}'",
            "✓".bold().green(),
            name
        );
    } else {
        println!(
            "{} {} capabilities would be affected:\n",
            "!".bold().yellow(),
            impacted.len()
        );
        for (cap, trace) in &impacted {
            let trace_str = trace.join(" → ");
            println!("  {} {}", cap.bold().red(), trace_str.dimmed());
        }
    }

    Ok(())
}

fn cmd_diff(base: &str, head: &str, path: &PathBuf, language: &Language) -> Result<()> {
    // Detect language from the original project dir before archiving
    let abs_path = std::fs::canonicalize(path)
        .with_context(|| format!("Cannot find directory: {}", path.display()))?;
    let resolved_lang = match language {
        Language::Auto => detect_language(&abs_path)?,
        other => other.clone(),
    };

    eprintln!("{} Comparing {} → {}...", "pecto".bold().cyan(), base, head);

    let temp_dir = std::env::temp_dir().join("pecto-diff");
    let base_dir = temp_dir.join("base");
    let head_dir = temp_dir.join("head");

    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&base_dir).context("Failed to create temp dir")?;
    std::fs::create_dir_all(&head_dir).context("Failed to create temp dir")?;

    export_git_ref(&abs_path, base, &base_dir)?;
    export_git_ref(&abs_path, head, &head_dir)?;

    let base_spec = analyze(&base_dir, &resolved_lang)?;
    let head_spec = analyze(&head_dir, &resolved_lang)?;

    let base_yaml =
        pecto_core::output::to_yaml(&base_spec).context("Failed to serialize base spec")?;
    let head_yaml =
        pecto_core::output::to_yaml(&head_spec).context("Failed to serialize head spec")?;

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

    let child = std::process::Command::new("tar")
        .args(["xf", "-"])
        .current_dir(target)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .context("Failed to run tar")?;

    use std::io::Write;
    let mut child = child;
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
