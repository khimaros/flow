use crate::node::{DataType, InputSpec, Node, NodeContext, OutputSpec, UIComponent};
use crate::value::Value;
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use scraper::{Html, Selector};
use std::collections::BTreeMap;
use url::Url;

/// fetches a URL and returns the raw content
pub struct WebFetchNode;

#[async_trait]
impl Node for WebFetchNode {
    fn name(&self) -> &str {
        "WebFetch"
    }

    fn title(&self) -> &str {
        "Web Fetch"
    }

    fn category(&self) -> &str {
        "Network"
    }

    fn description(&self) -> &str {
        "Fetches a URL and returns the raw HTML content."
    }

    fn inputs(&self) -> Vec<InputSpec> {
        vec![InputSpec {
            name: "url".to_string(),
            r#type: DataType::String,
            ui_component: UIComponent::Auto {},
            default: None,
            required: true,
            description: Some("the URL to fetch.".to_string()),
            ..Default::default()
        }]
    }

    fn outputs(&self) -> Vec<OutputSpec> {
        vec![
            OutputSpec {
                name: "content".to_string(),
                r#type: DataType::String,
                description: Some("the raw response content.".to_string()),
            },
            OutputSpec {
                name: "status".to_string(),
                r#type: DataType::Integer,
                description: Some("the HTTP status code.".to_string()),
            },
        ]
    }

    async fn execute(
        &self,
        inputs: BTreeMap<String, Value>,
        ctx: NodeContext,
    ) -> Result<BTreeMap<String, Value>> {
        let url = inputs
            .get("url")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("missing or invalid 'url' input"))?;

        let client = reqwest::Client::builder()
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
            .build()?;

        let cancel_token = ctx.cancel_token();
        let response = tokio::select! {
            res = client.get(url).send() => res?,
            _ = cancel_token.cancelled() => {
                return Err(anyhow!("request cancelled"));
            }
        };

        let status = response.status().as_u16() as i64;
        let content = response.text().await?;

        let mut outputs = BTreeMap::new();
        outputs.insert("content".to_string(), Value::String(content));
        outputs.insert("status".to_string(), Value::Integer(status));

        Ok(outputs)
    }
}

/// converts HTML to Markdown
pub struct HtmlToMarkdownNode;

#[async_trait]
impl Node for HtmlToMarkdownNode {
    fn name(&self) -> &str {
        "HtmlToMarkdown"
    }

    fn title(&self) -> &str {
        "HTML to Markdown"
    }

    fn category(&self) -> &str {
        "Data"
    }

    fn description(&self) -> &str {
        "Converts HTML content to Markdown format."
    }

    fn inputs(&self) -> Vec<InputSpec> {
        vec![
            InputSpec {
                name: "html".to_string(),
                r#type: DataType::String,
                ui_component: UIComponent::TextArea {},
                default: None,
                required: true,
                description: Some("the HTML content to convert.".to_string()),
                ..Default::default()
            },
            InputSpec {
                name: "max_length".to_string(),
                r#type: DataType::Integer,
                ui_component: UIComponent::Auto {},
                default: Some(Value::Integer(0)),
                required: false,
                description: Some(
                    "truncate markdown output to this many characters (0 = unlimited)."
                        .to_string(),
                ),
                ..Default::default()
            },
            InputSpec {
                name: "readability".to_string(),
                r#type: DataType::Boolean,
                ui_component: UIComponent::Auto {},
                default: Some(Value::Boolean(true)),
                required: false,
                description: Some(
                    "extract article content using readability. disable for comment threads or full-page conversion."
                        .to_string(),
                ),
                ..Default::default()
            },
        ]
    }

    fn outputs(&self) -> Vec<OutputSpec> {
        vec![
            OutputSpec {
                name: "markdown".to_string(),
                r#type: DataType::String,
                description: Some("the converted Markdown content.".to_string()),
            },
            OutputSpec {
                name: "title".to_string(),
                r#type: DataType::String,
                description: Some("the page title extracted from HTML.".to_string()),
            },
        ]
    }

    async fn execute(
        &self,
        inputs: BTreeMap<String, Value>,
        _ctx: NodeContext,
    ) -> Result<BTreeMap<String, Value>> {
        let html_content = inputs
            .get("html")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("missing or invalid 'html' input"))?;

        let use_readability = inputs
            .get("readability")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        let (title, clean_html) = if use_readability {
            let dummy_url = Url::parse("https://example.com").unwrap();
            match readability::extractor::extract(&mut html_content.as_bytes(), &dummy_url) {
                Ok(product) => (product.title, product.content),
                Err(_) => (extract_title(html_content), html_content.to_string()),
            }
        } else {
            (extract_title(html_content), html_content.to_string())
        };

        let converter = if use_readability {
            htmd::HtmlToMarkdown::new()
        } else {
            htmd::HtmlToMarkdown::builder()
                .skip_tags(vec!["script", "style", "svg", "noscript", "form"])
                .build()
        };
        let mut markdown = converter.convert(&clean_html).unwrap_or(clean_html);

        let max_length = inputs
            .get("max_length")
            .and_then(|v| v.as_i64())
            .unwrap_or(0) as usize;
        if max_length > 0 && markdown.len() > max_length {
            // truncate on a char boundary
            let mut end = max_length;
            while end > 0 && !markdown.is_char_boundary(end) {
                end -= 1;
            }
            markdown = format!("{}...", &markdown[..end]);
        }

        let mut outputs = BTreeMap::new();
        outputs.insert("markdown".to_string(), Value::String(markdown));
        outputs.insert("title".to_string(), Value::String(title));

        Ok(outputs)
    }
}

fn extract_title(html: &str) -> String {
    Html::parse_document(html)
        .select(&Selector::parse("title").unwrap())
        .next()
        .map(|el| el.text().collect::<String>())
        .unwrap_or_default()
}

/// performs a web search using DuckDuckGo
pub struct WebSearchNode;

#[async_trait]
impl Node for WebSearchNode {
    fn name(&self) -> &str {
        "WebSearch"
    }

    fn title(&self) -> &str {
        "Web Search"
    }

    fn category(&self) -> &str {
        "Network"
    }

    fn description(&self) -> &str {
        "Performs a web search using DuckDuckGo."
    }

    fn inputs(&self) -> Vec<InputSpec> {
        vec![
            InputSpec {
                name: "query".to_string(),
                r#type: DataType::String,
                ui_component: UIComponent::Auto {},
                default: Some(Value::String("".to_string())),
                required: true,
                description: Some("the search query.".to_string()),
                ..Default::default()
            },
            InputSpec {
                name: "max_results".to_string(),
                r#type: DataType::Integer,
                ui_component: UIComponent::Auto {},
                default: Some(Value::Integer(10)),
                required: false,
                description: Some("Maximum number of results to return.".to_string()),
                ..Default::default()
            },
        ]
    }

    fn outputs(&self) -> Vec<OutputSpec> {
        vec![OutputSpec {
            name: "json".to_string(),
            r#type: DataType::Object,
            description: Some(
                "JSON object with results array (title, url, snippet) and count.".to_string(),
            ),
        }]
    }

    async fn execute(
        &self,
        inputs: BTreeMap<String, Value>,
        ctx: NodeContext,
    ) -> Result<BTreeMap<String, Value>> {
        let query = inputs
            .get("query")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("missing or invalid 'query' input"))?;

        let max_results = inputs
            .get("max_results")
            .and_then(|v| v.as_i64())
            .unwrap_or(10) as usize;

        if query.trim().is_empty() {
            return Err(anyhow!("search query cannot be empty"));
        }

        let client = reqwest::Client::builder()
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
            .build()?;

        // use DuckDuckGo HTML search
        let search_url = format!(
            "https://html.duckduckgo.com/html/?q={}",
            urlencoding::encode(query)
        );

        let cancel_token = ctx.cancel_token();
        let response = tokio::select! {
            res = client.get(&search_url).send() => res?,
            _ = cancel_token.cancelled() => {
                return Err(anyhow!("search cancelled"));
            }
        };

        let html_content = response.text().await?;
        let document = Html::parse_document(&html_content);

        // parse DuckDuckGo search results
        let result_selector = Selector::parse(".result").unwrap();
        let title_selector = Selector::parse(".result__title a").unwrap();
        let snippet_selector = Selector::parse(".result__snippet").unwrap();
        let url_selector = Selector::parse(".result__url").unwrap();

        let mut results = Vec::new();

        for result in document.select(&result_selector).take(max_results) {
            let title = result
                .select(&title_selector)
                .next()
                .map(|el| el.text().collect::<String>().trim().to_string())
                .unwrap_or_default();

            let snippet = result
                .select(&snippet_selector)
                .next()
                .map(|el| el.text().collect::<String>().trim().to_string())
                .unwrap_or_default();

            // get the actual URL from the href attribute
            let url = result
                .select(&title_selector)
                .next()
                .and_then(|el| el.value().attr("href"))
                .map(|href| {
                    // duckDuckGo uses redirect URLs, extract the actual URL
                    if let Some(pos) = href.find("uddg=") {
                        let encoded = &href[pos + 5..];
                        if let Some(end) = encoded.find('&') {
                            urlencoding::decode(&encoded[..end])
                                .map(|s| s.to_string())
                                .unwrap_or_else(|_| encoded[..end].to_string())
                        } else {
                            urlencoding::decode(encoded)
                                .map(|s| s.to_string())
                                .unwrap_or_else(|_| encoded.to_string())
                        }
                    } else {
                        href.to_string()
                    }
                })
                .or_else(|| {
                    result
                        .select(&url_selector)
                        .next()
                        .map(|el| el.text().collect::<String>().trim().to_string())
                })
                .unwrap_or_default();

            if !title.is_empty() && !url.is_empty() {
                let mut result_obj = BTreeMap::new();
                result_obj.insert("title".to_string(), Value::String(title));
                result_obj.insert("url".to_string(), Value::String(url));
                result_obj.insert("snippet".to_string(), Value::String(snippet));
                results.push(Value::Object(result_obj));
            }
        }

        let count = results.len() as i64;

        // build the JSON object
        let mut json_obj = BTreeMap::new();
        json_obj.insert("results".to_string(), Value::Array(results));
        json_obj.insert("count".to_string(), Value::Integer(count));

        let mut outputs = BTreeMap::new();
        outputs.insert("json".to_string(), Value::Object(json_obj));

        Ok(outputs)
    }
}

// URL encoding helper since we need it for search queries
mod urlencoding {
    use std::borrow::Cow;

    pub fn encode(s: &str) -> String {
        let mut result = String::with_capacity(s.len() * 3);
        for c in s.chars() {
            match c {
                'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => result.push(c),
                ' ' => result.push('+'),
                _ => {
                    for byte in c.to_string().as_bytes() {
                        result.push_str(&format!("%{:02X}", byte));
                    }
                }
            }
        }
        result
    }

    pub fn decode(s: &str) -> Result<Cow<'_, str>, ()> {
        let mut result = String::with_capacity(s.len());
        let mut chars = s.chars().peekable();

        while let Some(c) = chars.next() {
            match c {
                '%' => {
                    let hex: String = chars.by_ref().take(2).collect();
                    if hex.len() == 2 {
                        if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                            result.push(byte as char);
                        } else {
                            result.push('%');
                            result.push_str(&hex);
                        }
                    } else {
                        result.push('%');
                        result.push_str(&hex);
                    }
                }
                '+' => result.push(' '),
                _ => result.push(c),
            }
        }

        Ok(Cow::Owned(result))
    }
}
