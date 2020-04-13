use colored::*;
use dirs;
use itertools::Itertools;
use project_configuration::{ProjectConfiguration, ProjectConfigurations};
use read_ctags::Language;
use serde_json;
use std::collections::HashSet;
use std::fs;
use std::io;
use std::iter::FromIterator;
use std::path::Path;
use structopt::StructOpt;
use token_analysis::*;
use token_search::{LanguageRestriction, TokenSearchConfig, TokenSearchResults};

#[derive(Debug, StructOpt)]
#[structopt(
    name = "unused-rs",
    about = "A command line tool to identify potentially unused code",
    setting = structopt::clap::AppSettings::ColoredHelp
)]
struct Flags {
    /// Disable color output
    #[structopt(long)]
    no_color: bool,

    /// Render output as JSON
    #[structopt(long)]
    json: bool,

    /// Hide progress bar
    #[structopt(long, short = "P")]
    no_progress: bool,

    /// Include tokens that fall into any likelihood category
    #[structopt(long, short = "a")]
    all_likelihoods: bool,

    /// Limit token output to those that match the provided likelihood(s)
    ///
    /// This allows for a comma-delimited list of likelihoods.
    #[structopt(long = "likelihood", short = "l", use_delimiter = true, default_value = "high", possible_values = &["high", "medium", "low"])]
    likelihoods: Vec<UsageLikelihoodStatus>,

    /// Sort output
    #[structopt(long, possible_values = &OrderField::variants(), default_value, case_insensitive = true)]
    sort_order: OrderField,

    /// Reverse sort order
    #[structopt(long)]
    reverse: bool,

    /// Limit tokens to those defined in the provided file extension(s)
    #[structopt(long, possible_values = &Language::extensions(), use_delimiter = true)]
    only_filetypes: Vec<Language>,

    /// Limit tokens to those defined except for the provided file extension(s)
    #[structopt(long, possible_values = &Language::extensions(), use_delimiter = true)]
    except_filetypes: Vec<Language>,
}

fn main() {
    let cmd = Flags::from_args();

    if cmd.no_color {
        control::set_override(false);
    }
    let search_config = build_token_search_config(&cmd);
    let analysis_filter = build_analysis_filter(&cmd);

    let results = TokenSearchResults::generate_with_config(&search_config);
    let config = calculate_config_by_results(&results).unwrap_or(ProjectConfiguration::default());

    let mut files_list = HashSet::new();
    let mut tokens_list = HashSet::new();

    let outcome = TokenUsageResults::calculate(&search_config, results, &config);

    if cmd.json {
        println!(
            "{}",
            serde_json::to_string(&outcome.filter(&analysis_filter)).unwrap()
        )
    } else {
        for analysis in outcome.filter(&analysis_filter) {
            let usage_likelihood = &analysis.usage_likelihood;
            tokens_list.insert(analysis.result.token.token.to_string());
            for v in analysis.result.occurrences.keys() {
                files_list.insert(v.to_string());
            }

            let display_token = match usage_likelihood.status {
                UsageLikelihoodStatus::High => analysis.result.token.token.red(),
                UsageLikelihoodStatus::Medium => analysis.result.token.token.yellow(),
                UsageLikelihoodStatus::Low => analysis.result.token.token.green(),
            };
            println!("{}", display_token);
            println!("   Reason: {}", usage_likelihood.reason.cyan());

            println!(
                "   Defined in: ({})",
                analysis.result.defined_paths().len().to_string().yellow()
            );
            for d in &analysis.result.defined_paths() {
                println!("   * {}", d.yellow());
            }

            let occurred_count = analysis.result.occurred_paths().len();

            if occurred_count > 0 {
                println!("   Found in: ({})", occurred_count.to_string().yellow());
                for d in &analysis.result.occurred_paths() {
                    println!("   * {}", d.yellow());
                }
            }

            println!("");
        }

        println!("");
        println!("{}", "== UNUSED SUMMARY ==".white());
        println!("   Tokens found: {}", colorize_total(tokens_list.len()));
        println!("   Files found: {}", colorize_total(files_list.len()));
        println!(
            "   Applied language filters: {}",
            format!("{}", search_config.language_restriction.to_string()).cyan()
        );
        println!(
            "   Sort order: {}",
            format!("{}", analysis_filter.sort_order).cyan()
        );
        println!(
            "   Usage likelihood: {}",
            analysis_filter
                .usage_likelihood_filter
                .iter()
                .map(|f| f.to_string())
                .join(", ")
                .cyan()
        );
        println!("   Configuration setting: {}", config.name.cyan());
        println!("");
    }
}

fn build_token_search_config(cmd: &Flags) -> TokenSearchConfig {
    let mut search_config = TokenSearchConfig::default();
    if cmd.no_progress {
        search_config.display_progress = false;
    }

    if !cmd.only_filetypes.is_empty() {
        search_config.language_restriction =
            LanguageRestriction::Only(to_hash_set(&cmd.only_filetypes));
    }

    if !cmd.except_filetypes.is_empty() {
        search_config.language_restriction =
            LanguageRestriction::Except(to_hash_set(&cmd.except_filetypes));
    }

    search_config
}

fn build_analysis_filter(cmd: &Flags) -> AnalysisFilter {
    let mut analysis_filter = AnalysisFilter::default();

    if !cmd.likelihoods.is_empty() {
        analysis_filter.usage_likelihood_filter = cmd.likelihoods.clone();
    }

    if cmd.all_likelihoods {
        analysis_filter.usage_likelihood_filter = vec![
            UsageLikelihoodStatus::High,
            UsageLikelihoodStatus::Medium,
            UsageLikelihoodStatus::Low,
        ];
    }

    analysis_filter.set_order_field(cmd.sort_order.clone());

    if cmd.reverse {
        analysis_filter.set_order_descending();
    }

    analysis_filter
}

fn colorize_total(amount: usize) -> colored::ColoredString {
    match amount {
        0 => "0".green(),
        _ => amount.to_string().red(),
    }
}

fn calculate_config_by_results(_results: &TokenSearchResults) -> Option<ProjectConfiguration> {
    let config_path: Option<String> = dirs::home_dir().and_then(|ref p| {
        let final_path = Path::new(p).join(".unused.yml");
        final_path.to_str().map(|v| v.to_owned())
    });
    match config_path {
        Some(path) => match read_file(&path) {
            Ok(contents) => ProjectConfigurations::load(&contents).get("Rails"),
            _ => None,
        },
        None => None,
    }
}

fn read_file(filename: &str) -> Result<String, io::Error> {
    let contents = fs::read_to_string(filename)?;

    Ok(contents)
}

fn to_hash_set<T>(input: &[T]) -> HashSet<T>
where
    T: std::hash::Hash + Eq + std::clone::Clone,
{
    HashSet::from_iter(input.iter().cloned())
}
