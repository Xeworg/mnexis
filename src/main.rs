use std::{path::PathBuf, sync::Arc, time::Duration};

use arrow_array::{
    ArrayRef, FixedSizeListArray, RecordBatch, RecordBatchIterator, StringArray, types::Float32Type,
};
use arrow_schema::{DataType, Field, Schema};
use clap::{Parser, Subcommand};
use futures::TryStreamExt;
use lancedb::query::{ExecutableQuery, QueryBase};
use mnexis_core::open_sqlite;
use serde::Deserialize;
use url::Url;

#[derive(Debug, Parser)]
#[command(name = "mnexis", version, about = "Mnexis local integration spike")]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Open local SQLite/LanceDB stores and probe loopback Ollama if available.
    Spike {
        #[arg(long, default_value = "./mnexis-data")]
        data_dir: PathBuf,
        #[arg(long, default_value = "http://127.0.0.1:11434")]
        ollama_url: String,
        #[arg(long, default_value = "embeddinggemma")]
        model: String,
    },
    /// Run a minimal MCP stdio ping tool.
    McpServe,
}

const OLLAMA_REQUEST_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Deserialize)]
struct EmbedResponse {
    embeddings: Vec<Vec<f32>>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    match Args::parse().command {
        Command::Spike {
            data_dir,
            ollama_url,
            model,
        } => run_spike(data_dir, &ollama_url, &model).await?,
        Command::McpServe => mnexis_core::mcp::serve_stdio().await?,
    }
    Ok(())
}

async fn run_spike(
    data_dir: PathBuf,
    ollama_url: &str,
    model: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    std::fs::create_dir_all(&data_dir)?;
    let sqlite_path = data_dir.join("mnexis.db");
    let connection = open_sqlite(&sqlite_path)?;
    println!("sqlite=ok path={}", sqlite_path.display());
    drop(connection);

    let vector_path = data_dir.join("vectors");
    let (search_rows, rows_after_delete) = run_vector_lifecycle(vector_path.clone()).await?;
    println!(
        "lancedb=ok path={} search_rows={search_rows} rows_after_delete={rows_after_delete} reopened=true",
        vector_path.display()
    );

    match probe_ollama(ollama_url, model).await {
        Ok(dimensions) => println!("ollama=ok model={model} dimensions={dimensions}"),
        Err(error) => eprintln!("ollama=unavailable reason={error}"),
    }
    Ok(())
}

async fn run_vector_lifecycle(
    vector_path: PathBuf,
) -> Result<(usize, usize), Box<dyn std::error::Error>> {
    const TABLE: &str = "mnexis_spike_vectors";
    let vector_uri = vector_path.to_string_lossy();
    let db = lancedb::connect(vector_uri.as_ref()).execute().await?;
    let table_names = db.table_names().execute().await?;
    let table = if table_names.iter().any(|name| name == TABLE) {
        db.open_table(TABLE).execute().await?
    } else {
        db.create_table(TABLE, vec![vector_batch([0.1, 0.2, 0.3])?])
            .execute()
            .await?
    };

    for _ in 0..2 {
        let schema = vector_schema();
        let batch = vector_batch([0.4, 0.5, 0.6])?;
        let source = RecordBatchIterator::new(vec![Ok(batch)], schema);
        let mut upsert = table.merge_insert(&["id"]);
        upsert
            .when_matched_update_all(None)
            .when_not_matched_insert_all();
        upsert.execute(Box::new(source)).await?;
    }

    let before_reopen = table.count_rows(Some("id = 'spike-1'".to_owned())).await?;
    if before_reopen != 1 {
        return Err(
            format!("idempotent vector upsert left {before_reopen} rows, expected 1").into(),
        );
    }
    drop(table);
    drop(db);

    let vector_uri = vector_path.to_string_lossy();
    let db = lancedb::connect(vector_uri.as_ref()).execute().await?;
    let table = db.open_table(TABLE).execute().await?;
    let batches: Vec<RecordBatch> = table
        .query()
        .nearest_to(&[0.4, 0.5, 0.6])?
        .limit(1)
        .execute()
        .await?
        .try_collect()
        .await?;
    let search_rows = batches.iter().map(RecordBatch::num_rows).sum();
    if search_rows != 1 {
        return Err(format!("vector search returned {search_rows} rows, expected 1").into());
    }

    table.delete("id = 'spike-1'").await?;
    let after_delete = table.count_rows(Some("id = 'spike-1'".to_owned())).await?;
    if after_delete != 0 {
        return Err(format!("vector delete left {after_delete} rows, expected 0").into());
    }
    drop(table);
    drop(db);

    let vector_uri = vector_path.to_string_lossy();
    let db = lancedb::connect(vector_uri.as_ref()).execute().await?;
    let table = db.open_table(TABLE).execute().await?;
    let after_reopen = table.count_rows(Some("id = 'spike-1'".to_owned())).await?;
    if after_reopen != 0 {
        return Err(format!("deleted vector reappeared after reopen: {after_reopen} rows").into());
    }
    Ok((search_rows, after_reopen))
}

fn vector_schema() -> Arc<Schema> {
    let dimension = 3;
    Arc::new(Schema::new(vec![
        Field::new("id", DataType::Utf8, false),
        Field::new(
            "vector",
            DataType::FixedSizeList(
                Arc::new(Field::new("item", DataType::Float32, true)),
                dimension,
            ),
            false,
        ),
    ]))
}

fn vector_batch(vector: [f32; 3]) -> Result<RecordBatch, arrow_schema::ArrowError> {
    let schema = vector_schema();
    let ids: ArrayRef = Arc::new(StringArray::from(vec!["spike-1"]));
    let vectors: ArrayRef = Arc::new(
        FixedSizeListArray::from_iter_primitive::<Float32Type, _, _>(
            [Some(vec![
                Some(vector[0]),
                Some(vector[1]),
                Some(vector[2]),
            ])],
            3,
        ),
    );
    RecordBatch::try_new(schema, vec![ids, vectors])
}

fn ollama_endpoint(base_url: &str) -> Result<Url, Box<dyn std::error::Error>> {
    let mut endpoint = Url::parse(base_url)?;
    if endpoint.scheme() != "http" {
        return Err("Ollama URL must use HTTP".into());
    }
    if !endpoint.username().is_empty() || endpoint.password().is_some() {
        return Err("Ollama URL must not contain credentials".into());
    }
    let address = match endpoint.host() {
        Some(url::Host::Ipv4(address)) => std::net::IpAddr::V4(address),
        Some(url::Host::Ipv6(address)) => std::net::IpAddr::V6(address),
        _ => return Err("Ollama URL must use a literal loopback IP address".into()),
    };
    if !address.is_loopback() {
        return Err("Ollama URL must use a loopback IP address".into());
    }
    endpoint.set_path("/api/embed");
    endpoint.set_query(None);
    endpoint.set_fragment(None);
    Ok(endpoint)
}

async fn probe_ollama(base_url: &str, model: &str) -> Result<usize, Box<dyn std::error::Error>> {
    let endpoint = ollama_endpoint(base_url)?;
    let client = reqwest::Client::builder()
        .timeout(OLLAMA_REQUEST_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let response = client
        .post(endpoint)
        .json(&serde_json::json!({ "model": model, "input": "Mnexis local integration probe" }))
        .send()
        .await?
        .error_for_status()?;
    let body: EmbedResponse = response.json().await?;
    let vector = body
        .embeddings
        .first()
        .ok_or("Ollama returned no embeddings")?;
    if vector.is_empty() || vector.iter().any(|value| !value.is_finite()) {
        return Err("Ollama returned an empty or non-finite embedding".into());
    }
    Ok(vector.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn ollama_probe_allows_a_slow_cold_start_response() {
        use std::io::{Read, Write};
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0_u8; 1024];
            stream.read(&mut request).unwrap();
            std::thread::sleep(Duration::from_secs(4));

            let body = r#"{"embeddings":[[0.1,0.2]]}"#;
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            )
            .unwrap();
        });

        let dimensions = probe_ollama(&format!("http://{address}"), "test-model")
            .await
            .unwrap();
        server.join().unwrap();
        assert_eq!(dimensions, 2);
    }

    #[test]
    fn ollama_endpoint_accepts_only_literal_loopback_http_hosts() {
        for url in ["http://127.0.0.1:11434", "http://[::1]:11434"] {
            assert!(
                ollama_endpoint(url).is_ok(),
                "expected loopback URL to pass: {url}"
            );
        }
        for url in [
            "http://localhost:11434",
            "http://192.0.2.1:11434",
            "https://127.0.0.1:11434",
            "http://user@127.0.0.1:11434",
        ] {
            assert!(
                ollama_endpoint(url).is_err(),
                "expected URL to be rejected: {url}"
            );
        }
    }
}
