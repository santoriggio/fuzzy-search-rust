use std::{
    fs::File,
    io::{BufRead, Read},
    time::Instant,
};

use rayon::prelude::*;

use actix_web::{
    web::{get, post, Data, Json},
    App, HttpResponse, HttpServer,
};
use serde::{Deserialize, Serialize};

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Leggi il file una sola volta all'avvio
    let names = get_names()?;
    let names_data = Data::new(names);

    let server = HttpServer::new(move || {
        App::new()
            .app_data(names_data.clone())
            .route("/", get().to(get_index))
            .route("/search", post().to(post_search))
    });

    println!("Serving on http://localhost:3000...");
    server.bind("127.0.0.1:3000")?.run().await?;

    Ok(())
}

async fn get_index() -> HttpResponse {
    HttpResponse::Ok().content_type("text/html").body(
        r#"
        <!DOCTYPE html>
        <html>
        <head>
            <title>Fuzzy Search</title>
            <style>
                body { font-family: Arial, sans-serif; margin: 40px; }
                .search-container { max-width: 600px; }
                input { width: 100%; padding: 10px; font-size: 16px; margin-bottom: 20px; }
                .result { padding: 8px; border-bottom: 1px solid #eee; }
                .distance { color: #666; font-size: 12px; }
                .time { color: #999; font-size: 11px; font-style: italic; }
                .stats { margin-bottom: 10px; color: #666; font-size: 14px; }
            </style>
        </head>
        <body>
            <div class="search-container">
                <h1>Fuzzy Search</h1>
                <input type="text" id="searchInput" placeholder="Inizia a digitare per cercare..." />
                <div id="stats"></div>
                <div id="results"></div>
            </div>
            
            <script>
                const searchInput = document.getElementById('searchInput');
                const resultsDiv = document.getElementById('results');
                const statsDiv = document.getElementById('stats');
                let timeoutId;
                
                searchInput.addEventListener('input', function() {
                    clearTimeout(timeoutId);
                    const query = this.value.trim();
                    
                    if (query.length === 0) {
                        resultsDiv.innerHTML = '';
                        statsDiv.innerHTML = '';
                        return;
                    }
                    
                    timeoutId = setTimeout(() => {
                        const startTime = performance.now();
                        
                        fetch('/search', {
                            method: 'POST',
                            headers: {
                                'Content-Type': 'application/json',
                            },
                            body: JSON.stringify({ query: query })
                        })
                        .then(response => response.json())
                        .then(data => {
                            const endTime = performance.now();
                            const clientTime = Math.round(endTime - startTime);
                            const serverTime = data.response_time || 0;
                            
                            statsDiv.innerHTML = `
                                <div class="stats">
                                    ${clientTime}ms
                                </div>
                            `;
                            
                            resultsDiv.innerHTML = '';
                            if (data.results.length === 0) {
                                resultsDiv.innerHTML = '<div class="result">Nessun risultato trovato</div>';
                            } else {
                                data.results.forEach(item => {
                                    const resultDiv = document.createElement('div');
                                    resultDiv.className = 'result';
                                    resultDiv.innerHTML = `
                                        <strong>${item.name}</strong>
                                        <span class="distance">(distanza: ${item.distance})</span>
                                    `;
                                    resultsDiv.appendChild(resultDiv);
                                });
                            }
                        })
                        .catch(error => {
                            console.error('Error:', error);
                            resultsDiv.innerHTML = '<div class="result">Errore nella ricerca</div>';
                            statsDiv.innerHTML = '';
                        });
                    }, 100);
                });
            </script>
        </body>
        </html>
        "#
    )
}

#[derive(Deserialize)]
struct SearchParams {
    query: String,
}

#[derive(Serialize, Debug)]
struct SearchResult {
    name: String,
    distance: usize,
}

#[derive(Serialize)]
struct SearchResponse {
    results: Vec<SearchResult>,
    response_time: u64,
}

async fn post_search(params: Json<SearchParams>, names: Data<Vec<String>>) -> HttpResponse {
    let start_time = Instant::now();

    let query = params.query.as_str();
    let query_bytes = query.as_bytes();

    let mut results: Vec<SearchResult> = names
        .par_iter()
        .filter_map(|name| {
            let distance = fuzzy_match(query_bytes, name);
            if distance < 3 {
                Some(SearchResult {
                    name: name.clone(),
                    distance,
                })
            } else {
                None
            }
        })
        .collect();

    results.sort_by_key(|item| item.distance);
    let response_time = start_time.elapsed().as_millis() as u64;

    HttpResponse::Ok().json(SearchResponse {
        results: results.into_iter().take(10).collect(),
        response_time,
    })
}

fn get_names() -> std::io::Result<Vec<String>> {
    let file = File::open("./names.csv")?;
    let reader = std::io::BufReader::new(file);
    let names = reader
        .lines()
        .filter_map(Result::ok)
        .filter(|line| !line.trim().is_empty())
        .collect();

    Ok(names)
}

#[inline(always)]
fn calc_dist_bytes(a: &[u8], b: &[u8]) -> usize {
    let len_a = a.len();
    let len_b = b.len();

    if len_a == 0 {
        return len_b;
    }
    if len_b == 0 {
        return len_a;
    }

    let mut prev: Vec<usize> = (0..=len_b).collect();
    let mut curr = vec![0; len_b + 1];

    for i in 0..len_a {
        curr[0] = i + 1;

        for j in 0..len_b {
            let cost = if a[i] == b[j] { 0 } else { 1 };

            curr[j + 1] = (prev[j + 1] + 1).min(curr[j] + 1).min(prev[j] + cost);
        }

        std::mem::swap(&mut prev, &mut curr);
    }

    prev[len_b]
}

fn fuzzy_match(query_bytes: &[u8], full_name: &str) -> usize {
    if !full_name.contains(' ') {
        return calc_dist_bytes(query_bytes, full_name.as_bytes());
    }

    full_name
        .split_whitespace()
        .map(|part| calc_dist_bytes(query_bytes, part.as_bytes()))
        .min()
        .unwrap_or(usize::MAX)
}
