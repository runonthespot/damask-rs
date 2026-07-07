mod app;
mod ck;
mod commands;
mod error;
mod output;

use clap::Parser;

use app::{Cli, Command};

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Init { claude, codex, no_agents } => commands::init::run(claude, codex, no_agents),
        Command::Bootstrap { force } => commands::bootstrap::run(force, cli.format),
        Command::Help { topic } => commands::help::run(topic.as_deref()),

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
            summary,
            confidence,
            action,
            severity,
            tags,
        } => commands::edge::run(
            &from,
            &to,
            &rel,
            payload.as_deref(),
            payload_file.as_deref(),
            stdin,
            summary.as_deref(),
            confidence,
            action.as_deref(),
            severity.as_deref(),
            &tags,
            cli.ns.as_deref(),
            cli.format,
        ),

        Command::Record {
            file,
            start,
            end,
            rel,
            payload,
            payload_file,
            stdin,
            summary,
            confidence,
            action,
            severity,
            tags,
            symbol,
            to,
        } => commands::record::run(
            &file,
            start,
            end,
            &rel,
            payload.as_deref(),
            payload_file.as_deref(),
            stdin,
            summary.as_deref(),
            confidence,
            action.as_deref(),
            severity.as_deref(),
            &tags,
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
            uncontested,
            show_closed,
            offset,
        } => commands::at::run(&location, cli.format, all, no_rank, rel.as_deref(), tag.as_deref(), uncontested, show_closed, offset),
        Command::Where {
            predicates,
            since,
            limit,
            offset,
            show_closed,
            sort,
        } => commands::where_cmd::run(&predicates, since.as_deref(), limit, offset, show_closed, sort, cli.format, cli.ns.as_deref()),
        Command::Follow { id, rel, depth } => {
            commands::follow::run(&id, rel.as_deref(), depth, cli.format)
        }
        Command::Endorse {
            edge_id,
            payload,
            batch,
        } => commands::endorse::run(
            edge_id.as_deref(),
            payload.as_deref(),
            batch,
            cli.ns.as_deref(),
        ),
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
        Command::Close {
            edge_id,
            payload,
            reason,
            batch,
        } => commands::close::run(
            edge_id.as_deref(),
            payload.as_deref(),
            reason.as_deref(),
            batch,
            cli.ns.as_deref(),
        ),
        Command::Orient {
            rel,
            tag,
            uncontested,
            show_closed,
        } => commands::orient::run(cli.format, rel.as_deref(), tag.as_deref(), uncontested, show_closed),
        Command::Briefing => commands::briefing::run(cli.format),
        Command::Peek {
            file,
            prompt,
            session,
        } => commands::peek::run(file.as_deref(), prompt.as_deref(), session.as_deref()),
        Command::Harvest { transcript } => commands::harvest::run(transcript.as_deref()),
        Command::Status => commands::status::run(cli.format),
        Command::Lint => commands::lint::run(cli.format),
        Command::Verify { auto, timeout } => {
            commands::verify::run(auto, timeout, cli.ns.as_deref(), cli.format)
        }
        Command::Compact {
            namespace,
            aggressive,
        } => commands::compact::run(namespace.as_deref(), aggressive),
        Command::Why { edge_id } => commands::why::run(&edge_id, cli.format),
        Command::Blame { id } => commands::blame::run(&id, cli.format),
        Command::Resolve { span_id } => commands::resolve::run(&span_id),
        Command::Log { limit, since } => commands::log::run(cli.format, limit, since.as_deref()),
        Command::Confirm { id } => commands::confirm::run(&id, cli.format),
        Command::Sweep { reanchor } => commands::sweep::run(reanchor, cli.format),
        Command::Tag { edge_id, tags } => commands::tag::run(&edge_id, &tags, cli.format),
        Command::Triage {
            close_deleted,
            close_refuted,
            close_ruled_out,
        } => commands::triage::run(close_deleted.as_deref(), close_refuted, close_ruled_out, cli.format),
        Command::Review { markdown } => commands::review::run(cli.format, markdown),
        Command::Search { query, ns, rel, where_preds, sem, limit, offset, show_closed } => {
            commands::search::run(&query, ns.as_deref(), rel.as_deref(), &where_preds, sem, limit, offset, show_closed, cli.format)
        }
        Command::Enrich => commands::enrich::run(cli.format),
        Command::Diff { ns_a, ns_b } => commands::diff::run(&ns_a, &ns_b, cli.format),
        Command::Tui => commands::tui::run(),
    }
}
