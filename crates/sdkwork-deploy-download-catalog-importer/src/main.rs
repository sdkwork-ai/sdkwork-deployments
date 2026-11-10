//! CLI entry point: plan a legacy download catalog, then optionally apply it.
//!
//! Dry run is the default because the importer writes to an authoritative table:
//! an operator reviews the report (counts, skipped installers, installers that
//! still need an update feed) before adding `--apply`. Applying is idempotent:
//! the repository upserts by catalog/card/installer key, so a second run patches
//! the same rows instead of duplicating them.

use std::path::PathBuf;
use std::process::ExitCode;

use sdkwork_deploy_download_catalog_importer::{plan_import, ImportOptions, ImportPlan};
use sdkwork_intelligence_deploy_repository_sqlx::DeployRepository;
use sdkwork_intelligence_deploy_service::DeployRepositoryPort;
use sqlx::postgres::PgPoolOptions;

struct Arguments {
    input: PathBuf,
    tenant_id: i64,
    catalog_key: String,
    product_key: Option<String>,
    channel: Option<String>,
    apply: bool,
}

fn usage() -> &'static str {
    "usage: download-catalog-importer --input <legacy.json> --tenant-id <id> --catalog-key <key> [--product-key <key>] [--channel <nightly|stable>] [--apply]"
}

fn parse_arguments(raw: impl Iterator<Item = String>) -> Result<Arguments, String> {
    let mut input: Option<PathBuf> = None;
    let mut tenant_id: Option<i64> = None;
    let mut catalog_key: Option<String> = None;
    let mut product_key: Option<String> = None;
    let mut channel: Option<String> = None;
    let mut apply = false;
    let mut pending: Vec<String> = raw.collect();
    let mut index = 0;
    while index < pending.len() {
        let flag = pending[index].clone();
        let mut value = || -> Result<String, String> {
            index += 1;
            pending
                .get(index)
                .cloned()
                .ok_or_else(|| format!("{flag} requires a value"))
        };
        match flag.as_str() {
            "--input" => input = Some(PathBuf::from(value()?)),
            "--tenant-id" => {
                tenant_id = Some(
                    value()?
                        .parse::<i64>()
                        .map_err(|error| format!("--tenant-id: {error}"))?,
                )
            }
            "--catalog-key" => catalog_key = Some(value()?),
            "--product-key" => product_key = Some(value()?),
            "--channel" => channel = Some(value()?),
            "--apply" => apply = true,
            "--help" | "-h" => return Err(usage().to_owned()),
            other => return Err(format!("unknown argument {other}\n{}", usage())),
        }
        index += 1;
    }
    Ok(Arguments {
        input: input.ok_or_else(|| "--input is required".to_owned())?,
        tenant_id: tenant_id.ok_or_else(|| "--tenant-id is required".to_owned())?,
        catalog_key: catalog_key.ok_or_else(|| "--catalog-key is required".to_owned())?,
        product_key,
        channel,
        apply,
    })
}

fn report(plan: &ImportPlan, apply: bool) -> serde_json::Value {
    serde_json::json!({
        "apply": apply,
        "catalog": plan.catalog,
        "cards": plan.cards.iter().map(|card| serde_json::json!({
            "cardKey": card.card_key,
            "installers": card.items.iter().map(|item| item.item_key.clone()).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        "installers": plan.cards.iter().map(|card| card.items.len()).sum::<usize>(),
        "installersWithoutUpdate": plan.installers_without_update,
        "warnings": plan.warnings,
    })
}

async fn apply_plan(plan: &ImportPlan, arguments: &Arguments) -> Result<(), String> {
    let url = std::env::var("SDKWORK_DATABASE_URL")
        .map_err(|_| "SDKWORK_DATABASE_URL is required to apply the import".to_owned())?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&url)
        .await
        .map_err(|error| format!("database connection failed: {error}"))?;
    // One-shot offline tool: node 0 and no certificate secret are safe because it
    // never runs beside a live writer and never touches certificate material.
    let repository = DeployRepository::new_lookup(pool);
    repository
        .upsert_download_catalog(
            arguments.tenant_id,
            None,
            &arguments.catalog_key,
            &plan.catalog,
        )
        .await
        .map_err(|error| format!("write catalog failed: {error}"))?;
    for card in &plan.cards {
        repository
            .upsert_download_card(
                arguments.tenant_id,
                None,
                &arguments.catalog_key,
                &card.card_key,
                &card.request,
            )
            .await
            .map_err(|error| format!("write card {} failed: {error}", card.card_key))?;
        for item in &card.items {
            repository
                .upsert_download_item(
                    arguments.tenant_id,
                    None,
                    &arguments.catalog_key,
                    &card.card_key,
                    &item.item_key,
                    &item.request,
                )
                .await
                .map_err(|error| {
                    format!(
                        "write installer {}/{} failed: {error}",
                        card.card_key, item.item_key
                    )
                })?;
        }
    }
    Ok(())
}

async fn run() -> Result<(), String> {
    let arguments = parse_arguments(std::env::args().skip(1))?;
    let document = tokio::fs::read_to_string(&arguments.input)
        .await
        .map_err(|error| format!("cannot read {}: {error}", arguments.input.display()))?;
    let plan = plan_import(
        &document,
        &ImportOptions {
            catalog_key: arguments.catalog_key.clone(),
            product_key: arguments.product_key.clone(),
            channel: arguments.channel.clone(),
        },
    )?;
    if arguments.apply {
        apply_plan(&plan, &arguments).await?;
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&report(&plan, arguments.apply))
            .map_err(|error| error.to_string())?
    );
    Ok(())
}

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}
