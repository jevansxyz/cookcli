use anyhow::{Context as AnyhowContext, Result};
use clap::Args;
use cooklang_import::url_to_recipe;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::Context;

#[derive(Debug, Args)]
pub struct GrabArgs {
    /// URL of the recipe webpage to grab and save
    ///
    /// Fetches the recipe from the URL, uses Claude AI to convert it to
    /// Cooklang format with metric units, downloads the recipe image,
    /// and saves everything to ~/cooklang/recipes/To Try/.
    ///
    /// Requires the ANTHROPIC_API_KEY environment variable to be set.
    ///
    /// Examples:
    ///   cook grab https://www.allrecipes.com/recipe/...
    ///   cook grab https://www.bbcgoodfood.com/recipes/...
    #[arg(value_name = "URL")]
    url: String,

    /// Output directory for the saved recipe and image
    ///
    /// Defaults to ~/cooklang/recipes/To Try
    #[arg(short, long, value_name = "DIR")]
    output: Option<PathBuf>,
}

#[derive(Serialize)]
struct ClaudeRequest {
    model: String,
    max_tokens: u32,
    messages: Vec<ClaudeMessage>,
}

#[derive(Serialize)]
struct ClaudeMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct ClaudeResponse {
    content: Vec<ClaudeContent>,
}

#[derive(Deserialize)]
struct ClaudeContent {
    text: String,
}

async fn call_claude(api_key: &str, recipe_name: &str, recipe_text: &str) -> Result<String> {
    let client = reqwest::Client::new();

    let prompt = format!(
        "Convert the following recipe to Cooklang format. Requirements:\n\
         - Use metric units throughout (grams, millilitres, litres, Celsius)\n\
         - Convert any imperial measurements to metric equivalents\n\
         - Use Cooklang syntax: @ingredient{{amount%unit}} for ingredients, \
           #cookware{{}} for cookware, ~timer{{time%unit}} for timers\n\
         - Do NOT include a title line at the top (the filename is the title)\n\
         - Output ONLY the Cooklang content with no explanation, preamble, or markdown code blocks\n\
         - Start directly with the recipe steps\n\n\
         Recipe name: {}\n\n\
         {}",
        recipe_name, recipe_text
    );

    let request = ClaudeRequest {
        model: "claude-opus-4-6".to_string(),
        max_tokens: 4096,
        messages: vec![ClaudeMessage {
            role: "user".to_string(),
            content: prompt,
        }],
    };

    let response = client
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&request)
        .send()
        .await
        .context("Failed to reach Claude API")?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("Claude API error {}: {}", status, body);
    }

    let claude_response: ClaudeResponse = response
        .json()
        .await
        .context("Failed to parse Claude API response")?;

    claude_response
        .content
        .into_iter()
        .next()
        .map(|c| c.text)
        .context("Empty response from Claude API")
}

async fn extract_image_url(page_url: &str) -> Option<String> {
    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (compatible; CookCLI/1.0)")
        .build()
        .ok()?;
    let html = client.get(page_url).send().await.ok()?.text().await.ok()?;

    // og:image with content before property
    let re1 =
        Regex::new(r#"<meta[^>]+content=["']([^"']+)["'][^>]+property=["']og:image["']"#).ok()?;
    if let Some(cap) = re1.captures(&html) {
        return Some(cap[1].to_string());
    }

    // og:image with property before content
    let re2 =
        Regex::new(r#"<meta[^>]+property=["']og:image["'][^>]+content=["']([^"']+)["']"#).ok()?;
    if let Some(cap) = re2.captures(&html) {
        return Some(cap[1].to_string());
    }

    None
}

fn mime_to_ext(content_type: &str) -> Option<String> {
    let ct = content_type.split(';').next()?.trim();
    match ct {
        "image/jpeg" => Some("jpg".to_string()),
        "image/png" => Some("png".to_string()),
        "image/webp" => Some("webp".to_string()),
        "image/gif" => Some("gif".to_string()),
        "image/avif" => Some("avif".to_string()),
        _ => None,
    }
}

async fn download_image(image_url: &str, output_dir: &Path, recipe_name: &str) -> Result<()> {
    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (compatible; CookCLI/1.0)")
        .build()
        .context("Failed to build HTTP client")?;

    let response = client
        .get(image_url)
        .send()
        .await
        .context("Failed to download image")?;

    let ext = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .and_then(mime_to_ext)
        .or_else(|| {
            image_url
                .split('?')
                .next()
                .and_then(|u| u.rsplit('.').next())
                .filter(|e| ["jpg", "jpeg", "png", "webp", "gif", "avif"].contains(e))
                .map(|e| if e == "jpeg" { "jpg" } else { e }.to_string())
        })
        .unwrap_or_else(|| "jpg".to_string());

    let image_path = output_dir.join(format!("{}.{}", recipe_name, ext));
    let bytes = response
        .bytes()
        .await
        .context("Failed to read image data")?;
    std::fs::write(&image_path, &bytes)
        .with_context(|| format!("Failed to write image: {}", image_path.display()))?;

    println!("Image saved to: {}", image_path.display());
    Ok(())
}

fn sanitize_filename(name: &str) -> String {
    let sanitized: String = name
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '-',
            c => c,
        })
        .collect();
    // Collapse whitespace
    sanitized.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn run(_ctx: &Context, args: GrabArgs) -> Result<()> {
    let api_key = std::env::var("ANTHROPIC_API_KEY").context(
        "ANTHROPIC_API_KEY environment variable not set.\n\
         Please set it with: export ANTHROPIC_API_KEY=your-key-here",
    )?;

    let output_dir = match args.output {
        Some(p) => p,
        None => {
            let user_dirs =
                directories::UserDirs::new().context("Could not determine home directory")?;
            user_dirs.home_dir().join("cooklang/recipes/To Try")
        }
    };

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        eprintln!("Fetching recipe from {}...", args.url);

        let recipe = url_to_recipe(&args.url)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to fetch recipe: {}", e))?;

        let recipe_name = if recipe.name.is_empty() {
            "Untitled Recipe".to_string()
        } else {
            recipe.name.clone()
        };

        eprintln!("Found: {}", recipe_name);
        eprintln!("Converting to Cooklang (metric) with Claude...");

        let cooklang_content = call_claude(&api_key, &recipe_name, &recipe.text).await?;

        std::fs::create_dir_all(&output_dir)
            .with_context(|| format!("Failed to create directory: {}", output_dir.display()))?;

        let safe_name = sanitize_filename(&recipe_name);
        let cook_path = output_dir.join(format!("{}.cook", safe_name));

        std::fs::write(&cook_path, &cooklang_content)
            .with_context(|| format!("Failed to write: {}", cook_path.display()))?;

        println!("Recipe saved to: {}", cook_path.display());

        eprintln!("Looking for recipe image...");
        match extract_image_url(&args.url).await {
            Some(image_url) => {
                eprintln!("Downloading image...");
                if let Err(e) = download_image(&image_url, &output_dir, &safe_name).await {
                    eprintln!("Warning: could not download image: {}", e);
                }
            }
            None => eprintln!("No image found."),
        }

        Ok(())
    })
}
