mod app;
mod commands;
mod error;
mod output;

use clap::Parser;

use app::{Cli, Command};

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Init => commands::init::run(),

        Command::Ns { action } => commands::ns::run(action, cli.format),

        Command::Span {
            file,
            start,
            end,
            symbol,
        } => commands::span::run(
            &file,
            start,
            end,
            symbol.as_deref(),
            cli.ns.as_deref(),
            cli.format,
        ),

        Command::Edge {
            from,
            to,
            rel,
            payload,
            payload_file,
            stdin,
        } => commands::edge::run(
            &from,
            &to,
            &rel,
            payload.as_deref(),
            payload_file.as_deref(),
            stdin,
            cli.ns.as_deref(),
            cli.format,
        ),

        Command::At {
            location,
            all,
            no_rank,
        } => commands::at::run(&location, cli.format, all, no_rank),
        Command::Where {
            predicate,
            since,
            limit,
        } => commands::where_cmd::run(&predicate, since.as_deref(), limit, cli.format),
        Command::Follow { id, rel, depth } => {
            commands::follow::run(&id, rel.as_deref(), depth, cli.format)
        }
        Command::Endorse { edge_id, payload } => {
            commands::endorse::run(&edge_id, payload.as_deref())
        }
        Command::Dispute { edge_id, payload } => commands::dispute::run(&edge_id, &payload),
        Command::Status => commands::status::run(cli.format),
        Command::Lint => commands::lint::run(cli.format),
        Command::Compact {
            namespace,
            aggressive,
        } => commands::compact::run(namespace.as_deref(), aggressive),
        Command::Why { edge_id } => commands::why::run(&edge_id, cli.format),
        Command::Blame { id } => commands::blame::run(&id, cli.format),
        Command::Resolve { span_id } => commands::resolve::run(&span_id),
        Command::Log => commands::log::run(cli.format),
        Command::Review => commands::review::run(cli.format),
        Command::Search { query, ns, rel } => {
            commands::search::run(&query, ns.as_deref(), rel.as_deref(), cli.format)
        }
        Command::Diff { ns_a, ns_b } => commands::diff::run(&ns_a, &ns_b, cli.format),
        Command::Tui => commands::tui::run(),
    }
}
