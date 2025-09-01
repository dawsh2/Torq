use anyhow::Result;
use clap::{Args, Parser, Subcommand};
use colored::Colorize;

mod cache;
mod query;
mod patterns;

use cache::SimpleCache;
use query::QueryEngine;
use patterns::{PatternDetector, quick_check};

#[derive(Parser)]
#[command(name = "rq")]
#[command(about = "Rust Query - Simple semantic grep for Rust codebases")]
#[command(version = "0.2.0")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize rq for current project
    Init,
    /// Update rustdoc cache
    Update {
        /// Update specific crate
        #[arg(long)]
        crate_name: Option<String>,
        /// Force update even if files haven't changed
        #[arg(short, long)]
        force: bool,
    },
    /// Find items by pattern
    Find(FindArgs),
    /// Find similar items using fuzzy matching
    Similar {
        /// Pattern to search for
        pattern: String,
        /// Similarity threshold (0.0-1.0)
        #[arg(short, long, default_value = "0.6")]
        threshold: f32,
    },
    /// Check if an item exists
    Check {
        /// Item name to check
        name: String,
    },
    /// Search documentation strings
    Docs {
        /// Pattern to search in docs
        pattern: String,
    },
    /// Show simple statistics
    Stats,
    /// Find usage examples from tests
    Examples {
        /// Item name to find examples for
        name: String,
    },
    /// Find what calls this function/type
    Callers {
        /// Function or type name
        name: String,
    },
    /// Find what this function/type calls
    Calls {
        /// Function or type name  
        name: String,
    },
    /// Show system overview from module documentation
    Overview,
    /// Find all trait definitions
    Traits {
        /// Filter by trait name pattern
        #[arg(short, long)]
        pattern: Option<String>,
    },
    /// Simple search across all documentation (shortcut for docs)
    Search {
        /// Search term
        term: String,
    },
    /// Open rustdoc webpage for crate or item
    Open {
        /// Crate name or item to open docs for
        #[arg(short, long)]
        crate_name: Option<String>,
        /// Item name to search for in docs
        item: Option<String>,
    },
    /// Check architectural patterns
    Patterns {
        /// Pattern to check (canonical_abi, unified_collector, mpsc, health)
        pattern: Option<String>,
        /// Show fix suggestions
        #[arg(short, long)]
        fix: bool,
    },
}

#[derive(Args)]
struct FindArgs {
    /// Pattern to search for
    pattern: String,
    /// Filter by item type (struct, enum, function, etc.)
    #[arg(short, long)]
    r#type: Option<String>,
    /// Filter by module path
    #[arg(short, long)]
    module: Option<String>,
    /// Show only public items
    #[arg(short, long)]
    public: bool,
    /// Filter by crate name
    #[arg(short, long)]
    crate_name: Option<String>,
    /// Use regex pattern matching
    #[arg(long)]
    regex: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let cache = SimpleCache::new()?;
    let query_engine = QueryEngine::new(cache);

    match cli.command {
        Commands::Init => {
            println!("Initializing rq for current project...");
            query_engine.init()?;
            println!("✅ Initialized rq cache");
            println!("\nNext steps:");
            println!(
                "  1. Run {} to build the initial cache",
                "rq update".green()
            );
            println!(
                "  2. Try {} to explore your codebase",
                "rq find <pattern>".green()
            );
        }

        Commands::Update { crate_name, force } => {
            let count = query_engine.update(crate_name.as_deref(), force)?;
            println!("✅ Updated {} crate(s)", count);
        }

        Commands::Find(args) => {
            let results = query_engine.find(
                &args.pattern,
                args.r#type.as_deref(),
                args.module.as_deref(),
                args.public,
                args.crate_name.as_deref(),
                args.regex,
            )?;

            if results.is_empty() {
                println!("No results found for '{}'", args.pattern);
            } else {
                for result in results {
                    println!(
                        "{}: {} ({})",
                        result.item_type.cyan(),
                        result.name.green(),
                        result.file.as_deref().unwrap_or("unknown").dimmed()
                    );
                    if let Some(docs) = result.docs {
                        if !docs.trim().is_empty() {
                            let first_line = docs.lines().next().unwrap_or(&docs);
                            println!("   {}", first_line.dimmed());
                        }
                    }
                }
            }
        }

        Commands::Similar { pattern, threshold } => {
            let results = query_engine.find_similar(&pattern, threshold)?;

            if results.is_empty() {
                println!("No similar items found for '{}'", pattern);
            } else {
                println!("Similar to '{}':", pattern);
                for result in results {
                    let similarity = result.similarity.unwrap_or(0.0);
                    println!(
                        "{}: {} ({:.0}% match) ({})",
                        result.item_type.cyan(),
                        result.name.green(),
                        similarity * 100.0,
                        result.file.as_deref().unwrap_or("unknown").dimmed()
                    );
                }
            }
        }

        Commands::Check { name } => {
            let exists = query_engine.check_exists(&name)?;
            if exists {
                println!("✅ {} exists", name.green());
            } else {
                println!("❌ {} not found", name.red());

                // Suggest alternatives
                let suggestions = query_engine.find_similar(&name, 0.5)?;
                if !suggestions.is_empty() {
                    println!("\nDid you mean:");
                    for suggestion in suggestions.into_iter().take(3) {
                        println!("  • {}", suggestion.name.yellow());
                    }
                }
            }
        }

        Commands::Docs { pattern } => {
            let results = query_engine.search_docs(&pattern)?;

            if results.is_empty() {
                println!("No documentation found containing '{}'", pattern);
            } else {
                for result in results {
                    println!("{}: {}", result.item_type.cyan(), result.name.green());
                    if let Some(docs) = result.docs {
                        println!(
                            "   {}",
                            docs.lines().take(3).collect::<Vec<_>>().join("\n   ")
                        );
                    }
                    println!();
                }
            }
        }

        Commands::Stats => {
            let stats = query_engine.get_stats()?;
            println!("📊 Cache Statistics:");
            println!("   Total items: {}", stats.total_items);
            println!("   Total crates: {}", stats.total_crates);
            if !stats.by_type.is_empty() {
                println!("\n   By type:");
                for (type_name, count) in stats.by_type {
                    println!("     {}: {}", type_name, count);
                }
            }
        }

        Commands::Examples { name } => {
            let examples = query_engine.find_examples(&name)?;

            if examples.is_empty() {
                println!("No test examples found for '{}'", name);
            } else {
                println!("Usage examples for {}:", name.green());
                for example in examples {
                    println!("\n📁 {}", example.file.cyan());
                    for line in example.code_lines.iter().take(5) {
                        println!("   {}", line.dimmed());
                    }
                    if example.code_lines.len() > 5 {
                        println!(
                            "   {} ... ({} more lines)",
                            "".dimmed(),
                            example.code_lines.len() - 5
                        );
                    }
                }
            }
        }

        Commands::Callers { name } => {
            let callers = query_engine.find_callers(&name)?;

            if callers.is_empty() {
                println!("No callers found for '{}'", name);
            } else {
                println!("Functions that call {}:", name.green());
                for caller in callers {
                    println!(
                        "{}: {} ({})",
                        caller.item_type.cyan(),
                        caller.name.green(),
                        caller.file.as_deref().unwrap_or("unknown").dimmed()
                    );
                }
            }
        }

        Commands::Calls { name } => {
            let calls = query_engine.find_calls(&name)?;

            if calls.is_empty() {
                println!("No calls found from '{}'", name);
            } else {
                println!("Functions called by {}:", name.green());
                for call in calls {
                    println!(
                        "{}: {} ({})",
                        call.item_type.cyan(),
                        call.name.green(),
                        call.file.as_deref().unwrap_or("unknown").dimmed()
                    );
                }
            }
        }

        Commands::Overview => {
            let overview = query_engine.get_system_overview()?;
            println!("{}", overview);
        }

        Commands::Traits { pattern } => {
            let results = query_engine.find_traits(pattern.as_deref())?;

            if results.is_empty() {
                println!("No traits found");
            } else {
                println!("Traits:");
                for result in results {
                    println!(
                        "{}: {} ({})",
                        "trait".cyan(),
                        result.name.green(),
                        result.file.as_deref().unwrap_or("unknown").dimmed()
                    );
                    if let Some(docs) = result.docs {
                        if !docs.trim().is_empty() {
                            let first_line = docs.lines().next().unwrap_or(&docs);
                            println!("   {}", first_line.dimmed());
                        }
                    }
                }
            }
        }

        Commands::Search { term } => {
            let results = query_engine.search_docs(&term)?;

            if results.is_empty() {
                println!("No documentation found containing '{}'", term);
            } else {
                for result in results {
                    println!("{}: {}", result.item_type.cyan(), result.name.green());
                    if let Some(docs) = result.docs {
                        let lines: Vec<&str> = docs.lines().collect();
                        let matching_lines: Vec<&str> = lines
                            .iter()
                            .filter(|line| line.to_lowercase().contains(&term.to_lowercase()))
                            .take(3)
                            .cloned()
                            .collect();

                        for line in matching_lines {
                            println!("   {}", line.dimmed());
                        }
                    }
                    println!();
                }
            }
        }

        Commands::Open { crate_name, item } => {
            if let Some(crate_name) = crate_name {
                // Open docs for specific crate
                let doc_path = format!("target/doc/{}/index.html", crate_name.replace('-', "_"));
                
                // Check if docs exist
                if std::path::Path::new(&doc_path).exists() {
                    println!("Opening documentation for crate '{}'...", crate_name);
                    std::process::Command::new("open").arg(&doc_path).spawn()?;
                } else {
                    println!("Documentation not found for '{}'. Try running 'cargo doc' first.", crate_name);
                }
            } else if let Some(item) = item {
                // Search for item and try to open its docs
                let results = query_engine.find(&item, None, None, false, None, false)?;
                
                if results.is_empty() {
                    println!("Item '{}' not found", item);
                } else {
                    // Take first result and try to open its documentation
                    let result = &results[0];
                    let crate_name = &result.crate_name;
                    let doc_path = format!("target/doc/{}/index.html", crate_name.replace('-', "_"));
                    
                    if std::path::Path::new(&doc_path).exists() {
                        println!("Opening documentation for '{}' in crate '{}'...", item, crate_name);
                        std::process::Command::new("open").arg(&doc_path).spawn()?;
                    } else {
                        println!("Documentation not found. Try running 'cargo doc' first.");
                    }
                }
            } else {
                // Open main workspace docs - try common rustdoc entry points
                let doc_paths = [
                    "target/doc/index.html",
                    "target/doc/protocol_v2/index.html",  // Protocol is the main crate
                    "target/doc/torq_relays/index.html",  // Relays are core infrastructure
                ];
                
                for doc_path in &doc_paths {
                    if std::path::Path::new(doc_path).exists() {
                        println!("Opening documentation: {}", doc_path);
                        std::process::Command::new("open").arg(doc_path).spawn()?;
                        return Ok(());
                    }
                }
                
                println!("Workspace documentation not found. Try running 'cargo doc' first.");
            }
        }
        
        Commands::Patterns { pattern, fix } => {
            if fix {
                println!("{}", "📝 Fix Suggestions:".bold().blue());
                println!("\n{}", "For MPSC issues:".bold());
                println!("  1. Remove MPSC channel creation");
                println!("  2. Use RelayOutput directly");
                println!("  3. See bin/polygon/polygon.rs for reference");
                
                println!("\n{}", "For duplicate ABI:".bold());
                println!("  1. Delete duplicate event definitions");
                println!("  2. Import from torq_dex");
                println!("  3. Use torq_dex::get_all_event_signatures()");
                
                println!("\n{}", "For legacy collectors:".bold());
                println!("  1. Combine collector and publisher logic");
                println!("  2. Move to services/adapters/src/bin/");
                println!("  3. Follow unified pattern from polygon.rs");
            } else if let Some(pattern) = pattern {
                quick_check(&query_engine, &pattern)?;
            } else {
                // Default to health check
                let detector = PatternDetector::new(&query_engine);
                detector.health_check()?;
            }
        }
    }

    Ok(())
}
