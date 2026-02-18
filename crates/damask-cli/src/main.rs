mod app;
mod commands;
mod error;
mod output;

use clap::Parser;

use app::{Cli, Command};

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Init { claude, codex } => commands::init::run(claude, codex),

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

        Command::Record {
            file,
            start,
            end,
            rel,
            payload,
            symbol,
            to,
        } => commands::record::run(
            &file,
            start,
            end,
            &rel,
            &payload,
            symbol.as_deref(),
            &to,
            cli.ns.as_deref(),
            cli.format,
        ),

        Command::Batch { stdin, file } => {
            commands::batch::run(stdin, file.as_deref(), cli.ns.as_deref(), cli.format)
        }

        Command::At {
            location,
            all,
            no_rank,
            rel,
            tag,
            undisputed,
        } => commands::at::run(&location, cli.format, all, no_rank, rel.as_deref(), tag.as_deref(), undisputed),
        Command::Where {
            predicates,
            since,
            limit,
        } => commands::where_cmd::run(&predicates, since.as_deref(), limit, cli.format, cli.ns.as_deref()),
        Command::Follow { id, rel, depth } => {
            commands::follow::run(&id, rel.as_deref(), depth, cli.format)
        }
        Command::Endorse { edge_id, payload } => {
            commands::endorse::run(&edge_id, payload.as_deref(), cli.ns.as_deref())
        }
        Command::Dispute {
            edge_id,
            payload,
            reason,
            batch,
        } => commands::dispute::run(
            edge_id.as_deref(),
            payload.as_deref(),
            reason.as_deref(),
            batch,
            cli.ns.as_deref(),
        ),
        Command::Orient {
            rel,
            tag,
            undisputed,
        } => commands::orient::run(cli.format, rel.as_deref(), tag.as_deref(), undisputed),
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
