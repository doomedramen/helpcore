use std::collections::{HashMap, HashSet};

// ── Data structures ────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct Plugin {
    name: String,
    brief: String,
    domain_tags: Vec<String>,
    is_always_useful: bool,
}

#[derive(Debug, serde::Serialize)]
struct ValidationReport {
    plugins: Vec<PluginResult>,
    ambiguity_pairs: Vec<AmbiguityPair>,
    domain_clusters: Vec<DomainCluster>,
    summary: Summary,
}

#[derive(Debug, serde::Serialize)]
struct PluginResult {
    name: String,
    brief: String,
    confidence: Confidence,
    word_count: usize,
    has_verb: bool,
    has_concrete_nouns: bool,
    is_always_useful: bool,
    notes: Vec<String>,
    overlapping_with: Vec<String>,
}

#[derive(Debug, serde::Serialize)]
struct AmbiguityPair {
    plugin_a: String,
    plugin_b: String,
    shared_words: Vec<String>,
    overlap_ratio: f64,
    risk: Risk,
}

#[derive(Debug, serde::Serialize)]
struct DomainCluster {
    domain: String,
    plugins: Vec<String>,
    notes: String,
}

#[derive(Debug, serde::Serialize)]
struct Summary {
    total_plugins: usize,
    high_confidence: usize,
    medium_confidence: usize,
    low_confidence: usize,
    ambiguous_pairs: usize,
    always_useful_plugins: Vec<String>,
    needs_author_review: Vec<String>,
    estimated_context_savings_pct: f64,
}

#[derive(Debug, Clone, Copy, serde::Serialize, PartialEq)]
enum Confidence {
    High,
    Medium,
    Low,
}

#[derive(Debug, Clone, Copy, serde::Serialize)]
#[allow(dead_code)]
enum Risk {
    None,
    Low,
    Medium,
    High,
}

// ── Stopwords for word overlap comparison ──────────────────────────────────

const STOPWORDS: &[&str] = &[
    "the", "a", "an", "is", "are", "was", "were", "be", "been", "being", "have", "has", "had",
    "do", "does", "did", "will", "would", "could", "should", "may", "might", "can", "shall", "to",
    "of", "in", "for", "on", "with", "at", "by", "from", "as", "into", "through", "during",
    "before", "after", "above", "below", "between", "under", "again", "then", "once", "here",
    "there", "when", "where", "why", "how", "all", "both", "each", "few", "more", "most", "other",
    "some", "such", "no", "nor", "not", "only", "own", "same", "so", "than", "too", "very", "and",
    "but", "or", "if", "because", "until", "while", "about", "also", "up", "out", "just", "now",
    "its", "it", "use", "when", "user", "asks", "about", "any", "their", "they",
];

// ── Plugin manifest ────────────────────────────────────────────────────────

fn build_plugins() -> Vec<Plugin> {
    vec![
        Plugin {
            name: "calculator".into(),
            brief: "Use when the user asks to compute, calculate, solve, or evaluate a math expression, percentage, or numeric conversion.".into(),
            domain_tags: vec!["utility".into(), "math".into()],
            is_always_useful: true,
        },
        Plugin {
            name: "color-tools".into(),
            brief: "Use when the user asks to convert between color formats, generate color palettes or harmonies, or identify a color name from a hex value.".into(),
            domain_tags: vec!["design".into(), "utility".into()],
            is_always_useful: false,
        },
        Plugin {
            name: "country-info".into(),
            brief: "Use when the user asks for facts about a specific country — population, capital, currency, language, timezone, or border information.".into(),
            domain_tags: vec!["reference".into(), "geography".into()],
            is_always_useful: false,
        },
        Plugin {
            name: "currency-lens".into(),
            brief: "Use when the user asks to convert an amount between currencies or check the current exchange rate between two currencies.".into(),
            domain_tags: vec!["finance".into(), "utility".into()],
            is_always_useful: false,
        },
        Plugin {
            name: "data-format".into(),
            brief: "Use when the user needs to convert data between formats — CSV, JSON, YAML, XML, or Markdown to HTML.".into(),
            domain_tags: vec!["utility".into(), "development".into()],
            is_always_useful: false,
        },
        Plugin {
            name: "date-utils".into(),
            brief: "Use when the user asks for date arithmetic, weekday of a date, week number, Unix timestamp conversion, or leap year check.".into(),
            domain_tags: vec!["utility".into(), "time".into()],
            is_always_useful: false,
        },
        Plugin {
            name: "dictionary".into(),
            brief: "Use when the user asks for the definition, pronunciation, synonyms, or antonyms of an English word.".into(),
            domain_tags: vec!["reference".into(), "language".into()],
            is_always_useful: false,
        },
        Plugin {
            name: "dns-peek".into(),
            brief: "Use when the user asks about DNS records for a domain — IP address, mail server, name server, or TXT records.".into(),
            domain_tags: vec!["networking".into(), "utility".into()],
            is_always_useful: false,
        },
        Plugin {
            name: "duckduckgo".into(),
            brief: "Use when the user asks for current information, recent events, web search results, or anything requiring live internet knowledge beyond your training data.".into(),
            domain_tags: vec!["search".into(), "reference".into()],
            is_always_useful: true,
        },
        Plugin {
            name: "finance".into(),
            brief: "Use when the user asks for a stock price by ticker symbol or a cryptocurrency price by name or symbol.".into(),
            domain_tags: vec!["finance".into()],
            is_always_useful: false,
        },
        Plugin {
            name: "holiday-check".into(),
            brief: "Use when the user asks about public holidays — what holidays a country has in a given year or what holidays are coming up.".into(),
            domain_tags: vec!["reference".into(), "time".into()],
            is_always_useful: false,
        },
        Plugin {
            name: "homeassistant".into(),
            brief: "Use when the user asks to control smart home devices — lights, thermostats, locks, covers, fans, or to trigger automations, scenes, or scripts.".into(),
            domain_tags: vec!["home-automation".into(), "iot".into()],
            is_always_useful: false,
        },
        Plugin {
            name: "image-info".into(),
            brief: "Use when the user provides an image URL or base64 data and asks about image format, dimensions, color mode, or file size.".into(),
            domain_tags: vec!["media".into(), "utility".into()],
            is_always_useful: false,
        },
        Plugin {
            name: "markdown-utils".into(),
            brief: "Use when the user asks to strip Markdown formatting to plain text, or extract links, headings, or code blocks from Markdown content.".into(),
            domain_tags: vec!["utility".into(), "development".into()],
            is_always_useful: false,
        },
        Plugin {
            name: "meal-finder".into(),
            brief: "Use when the user asks for recipes, meal ideas by ingredient, weekly meal plans, or shopping lists based on meals.".into(),
            domain_tags: vec!["food".into(), "lifestyle".into()],
            is_always_useful: false,
        },
        Plugin {
            name: "movies-tv".into(),
            brief: "Use when the user asks about movies or TV shows — searching for titles, getting ratings, cast, plot summaries, or season details.".into(),
            domain_tags: vec!["media".into(), "entertainment".into()],
            is_always_useful: false,
        },
        Plugin {
            name: "net-identity".into(),
            brief: "Use when the user asks about their IP address, where they are connecting from, their ISP, or to look up location info for a specific IP.".into(),
            domain_tags: vec!["networking".into(), "utility".into()],
            is_always_useful: false,
        },
        Plugin {
            name: "nutrition-facts".into(),
            brief: "Use when the user asks for nutritional information — calories, protein, fat, carbs of a food, product nutrition by barcode, or comparing two foods.".into(),
            domain_tags: vec!["food".into(), "health".into()],
            is_always_useful: false,
        },
        Plugin {
            name: "page-fetch".into(),
            brief: "Use when the user shares a URL and asks to summarize, extract information from, or answer questions about a web page content.".into(),
            domain_tags: vec!["web".into(), "utility".into()],
            is_always_useful: false,
        },
        Plugin {
            name: "plex".into(),
            brief: "Use when the user asks about their Plex media server — searching for movies, shows, or music, recently added, currently playing, or library contents.".into(),
            domain_tags: vec!["media".into(), "home-server".into()],
            is_always_useful: false,
        },
        Plugin {
            name: "pollen-count".into(),
            brief: "Use when the user asks about pollen levels or allergy conditions for a location in Europe.".into(),
            domain_tags: vec!["weather".into(), "health".into()],
            is_always_useful: false,
        },
        Plugin {
            name: "productivity".into(),
            brief: "Use when the user asks for a pomodoro timer, random pick from a list, dice roll, coin flip, random number, or password generation.".into(),
            domain_tags: vec!["utility".into(), "lifestyle".into()],
            is_always_useful: false,
        },
        Plugin {
            name: "proxmox".into(),
            brief: "Use when the user asks to manage their Proxmox VE server — checking node, VM, or container status, resource usage, storage, or cluster health.".into(),
            domain_tags: vec!["home-server".into(), "infrastructure".into()],
            is_always_useful: false,
        },
        Plugin {
            name: "qr-code".into(),
            brief: "Use when the user asks to generate a QR code from text, a URL, or any content.".into(),
            domain_tags: vec!["utility".into()],
            is_always_useful: false,
        },
        Plugin {
            name: "radarr".into(),
            brief: "Use when the user asks to search for, add, or manage movies in their Radarr download server.".into(),
            domain_tags: vec!["media".into(), "home-server".into(), "download".into()],
            is_always_useful: false,
        },
        Plugin {
            name: "random-facts".into(),
            brief: "Use when the user asks for a random joke, fun fact, or inspirational quote — for entertainment, not factual reference.".into(),
            domain_tags: vec!["entertainment".into()],
            is_always_useful: false,
        },
        Plugin {
            name: "recommend".into(),
            brief: "Use when the user asks for content recommendations — similar music artists, movies, TV shows, books, authors, games, or podcasts.".into(),
            domain_tags: vec!["media".into(), "entertainment".into()],
            is_always_useful: false,
        },
        Plugin {
            name: "seerr".into(),
            brief: "Use when the user asks to search for, request, or discover movies and TV shows through Overseerr or Jellyseerr.".into(),
            domain_tags: vec!["media".into(), "home-server".into(), "download".into()],
            is_always_useful: false,
        },
        Plugin {
            name: "sonarr".into(),
            brief: "Use when the user asks to search for, add, or manage TV shows in their Sonarr download server.".into(),
            domain_tags: vec!["media".into(), "home-server".into(), "download".into()],
            is_always_useful: false,
        },
        Plugin {
            name: "sun-times".into(),
            brief: "Use when the user asks about sunrise, sunset, twilight times, or daylight hours for a location.".into(),
            domain_tags: vec!["weather".into(), "time".into()],
            is_always_useful: false,
        },
        Plugin {
            name: "text-utils".into(),
            brief: "Use when the user asks to encode or decode base64, hash text, generate UUIDs, format or validate JSON, count text, convert case, slugify, or extract URLs and emails.".into(),
            domain_tags: vec!["utility".into(), "development".into()],
            is_always_useful: true,
        },
        Plugin {
            name: "translation".into(),
            brief: "Use when the user asks to translate text between languages or detect what language a text is written in.".into(),
            domain_tags: vec!["language".into(), "utility".into()],
            is_always_useful: false,
        },
        Plugin {
            name: "unit-converter".into(),
            brief: "Use when the user asks to convert between units — length, weight, temperature, volume, speed, data storage, area, time, pressure, energy, or angle.".into(),
            domain_tags: vec!["utility".into(), "math".into()],
            is_always_useful: false,
        },
        Plugin {
            name: "voice".into(),
            brief: "Use when the user provides an audio file or URL to transcribe, or asks for a spoken text-to-speech response to be generated.".into(),
            domain_tags: vec!["media".into(), "utility".into()],
            is_always_useful: false,
        },
        Plugin {
            name: "weather".into(),
            brief: "Use when the user asks about current conditions, temperature, or weather forecast for any location.".into(),
            domain_tags: vec!["weather".into()],
            is_always_useful: false,
        },
        Plugin {
            name: "wiki-brief".into(),
            brief: "Use when the user asks for a quick Wikipedia summary — who someone is, what something is, or a brief explanation of a topic.".into(),
            domain_tags: vec!["reference".into()],
            is_always_useful: false,
        },
        Plugin {
            name: "world-clock".into(),
            brief: "Use when the user asks what time it is in a specific city or country, to convert a time between timezones, or about timezone differences.".into(),
            domain_tags: vec!["time".into(), "utility".into()],
            is_always_useful: false,
        },
    ]
}

// ── Brief quality checks ───────────────────────────────────────────────────

fn meaningful_words(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .map(|w| w.trim().to_string())
        .filter(|w| w.len() > 2 && !STOPWORDS.contains(&w.as_str()))
        .collect()
}

fn has_verb(brief: &str) -> bool {
    // Check for action verbs that indicate user intent triggers
    let verbs = [
        "asks",
        "provides",
        "shares",
        "needs",
        "wants",
        "requests",
        "compute",
        "calculate",
        "convert",
        "generate",
        "search",
        "look",
        "check",
        "manage",
        "control",
        "fetch",
        "translate",
        "detect",
        "extract",
        "format",
        "validate",
        "compare",
        "identify",
    ];
    let lower = brief.to_lowercase();
    verbs.iter().any(|v| lower.contains(v))
}

fn has_concrete_nouns(brief: &str) -> bool {
    let lower = brief.to_lowercase();
    lower.contains("temperature")
        || lower.contains("forecast")
        || lower.contains("currency")
        || lower.contains("exchange rate")
        || lower.contains("recipe")
        || lower.contains("meal")
        || lower.contains("movie")
        || lower.contains("tv show")
        || lower.contains("film")
        || lower.contains("stock")
        || lower.contains("crypto")
        || lower.contains("definition")
        || lower.contains("pronunciation")
        || lower.contains("dns")
        || lower.contains("domain")
        || lower.contains("pollen")
        || lower.contains("allergy")
        || lower.contains("ip address")
        || lower.contains("isp")
        || lower.contains("pomodoro")
        || lower.contains("dice")
        || lower.contains("qr code")
        || lower.contains("sunrise")
        || lower.contains("sunset")
        || lower.contains("twilight")
        || lower.contains("timezone")
        || lower.contains("light")
        || lower.contains("thermostat")
        || lower.contains("smart home")
        || lower.contains("virtual machine")
        || lower.contains("vm")
        || lower.contains("container")
        || lower.contains("plex")
        || lower.contains("radarr")
        || lower.contains("sonarr")
        || lower.contains("seerr")
        || lower.contains("overseerr")
        || lower.contains("jellyseerr")
        || lower.contains("holiday")
        || lower.contains("public")
        || lower.contains("wikip")
        || lower.contains("summary")
        || lower.contains("color")
        || lower.contains("palette")
        || lower.contains("hex")
        || lower.contains("country")
        || lower.contains("population")
        || lower.contains("capital")
        || lower.contains("base64")
        || lower.contains("hash")
        || lower.contains("uuid")
        || lower.contains("audio")
        || lower.contains("transcribe")
        || lower.contains("speech")
        || lower.contains("format")
        || lower.contains("json")
        || lower.contains("yaml")
        || lower.contains("joke")
        || lower.contains("quote")
        || lower.contains("fact")
        || lower.contains("recommend")
        || lower.contains("similar")
        || lower.contains("nutrition")
        || lower.contains("calorie")
        || lower.contains("barcode")
        || lower.contains("image")
        || lower.contains("dimension")
        || lower.contains("html")
        || lower.contains("markdown")
        || lower.contains("date")
        || lower.contains("weekday")
        || lower.contains("leap")
        || lower.contains("unit")
        || lower.contains("length")
        || lower.contains("weight")
        || lower.contains("translate")
        || lower.contains("language")
        || lower.contains("password")
}

fn word_overlap(a: &[String], b: &[String]) -> (Vec<String>, f64) {
    let set_a: HashSet<_> = a.iter().collect();
    let set_b: HashSet<_> = b.iter().collect();
    let intersection: Vec<_> = set_a.intersection(&set_b).map(|s| s.to_string()).collect();
    let union = set_a.union(&set_b).count();
    let ratio = if union == 0 {
        0.0
    } else {
        intersection.len() as f64 / union as f64
    };
    (intersection, ratio)
}

// ── Domain collision analysis ──────────────────────────────────────────────

fn analyze_domain_clusters(plugins: &[Plugin]) -> Vec<DomainCluster> {
    let mut clusters: HashMap<String, Vec<String>> = HashMap::new();

    for p in plugins {
        for tag in &p.domain_tags {
            clusters
                .entry(tag.clone())
                .or_default()
                .push(p.name.clone());
        }
    }

    let notes = HashMap::from([
        ("media", "Movies-tv, plex, radarr, sonarr, seerr, recommend — 6 plugins in media domain. High collision risk for generic 'find me a movie' queries. Needs discriminative briefs that mention specific server/product names."),
        ("home-server", "Plex, radarr, sonarr, seerr, proxmox — 5 home-server tools. Risk: 'check my server' is ambiguous. Briefs must mention product names."),
        ("utility", "Calculator, color-tools, currency-lens, data-format, date-utils, dns-peek, image-info, markdown-utils, net-identity, page-fetch, productivity, qr-code, text-utils, unit-converter, voice, world-clock — 16 utility plugins. Cannot distinguish on domain alone. Each brief must have specific triggering verbs and objects."),
        ("weather", "Pollen-count, sun-times, weather — 3 weather-adjacent plugins. Weather and pollen-count are distinct enough; sun-times has sharper scope."),
        ("food", "Meal-finder and nutrition-facts — 2 overlapping. 'How healthy is lasagna' could trigger either. Briefs should distinguish recipe vs. nutrition data."),
    ]);

    clusters
        .into_iter()
        .filter(|(_, v)| v.len() >= 2)
        .map(|(domain, plugins)| DomainCluster {
            notes: notes
                .get(domain.as_str())
                .map(|s| s.to_string())
                .unwrap_or_default(),
            domain,
            plugins,
        })
        .collect()
}

// ── Main validation ────────────────────────────────────────────────────────

fn validate(plugins: &[Plugin]) -> ValidationReport {
    let mut results = Vec::new();
    let mut ambiguity_pairs = Vec::new();
    let stems: HashMap<String, Vec<String>> = plugins
        .iter()
        .map(|p| (p.name.clone(), meaningful_words(&p.brief)))
        .collect();

    // Per-plugin analysis
    for (i, p) in plugins.iter().enumerate() {
        let words = &stems[&p.name];
        let mut notes = Vec::new();
        let mut overlapping_with = Vec::new();

        let verb_ok = has_verb(&p.brief);
        let nouns_ok = has_concrete_nouns(&p.brief);

        if !verb_ok {
            notes.push("Missing action verb — brief may not activate on user intent".into());
        }
        if !nouns_ok {
            notes.push(
                "Missing concrete domain nouns — brief may be too vague to distinguish".into(),
            );
        }
        if p.is_always_useful {
            notes.push(
                "Always-useful plugin — risks being fetched too often. Brief must be narrow."
                    .into(),
            );
        }
        if p.brief.len() > 250 {
            notes.push(format!(
                "Brief is {} chars — aim for 100-180 chars for index density",
                p.brief.len()
            ));
        }

        // Check overlap with all other plugins
        for (j, other) in plugins.iter().enumerate() {
            if i == j {
                continue;
            }
            let other_words = &stems[&other.name];
            let (shared, ratio) = word_overlap(words, other_words);
            if ratio > 0.35 {
                overlapping_with.push(other.name.clone());
            }
            if ratio > 0.5 {
                ambiguity_pairs.push(AmbiguityPair {
                    plugin_a: p.name.clone(),
                    plugin_b: other.name.clone(),
                    shared_words: shared,
                    overlap_ratio: ratio,
                    risk: if ratio > 0.7 {
                        Risk::High
                    } else {
                        Risk::Medium
                    },
                });
            }
        }

        let confidence = if verb_ok && nouns_ok && overlapping_with.is_empty() {
            Confidence::High
        } else if verb_ok && nouns_ok && overlapping_with.len() <= 1 {
            Confidence::Medium
        } else {
            Confidence::Low
        };

        results.push(PluginResult {
            name: p.name.clone(),
            brief: p.brief.clone(),
            confidence,
            word_count: words.len(),
            has_verb: verb_ok,
            has_concrete_nouns: nouns_ok,
            is_always_useful: p.is_always_useful,
            notes,
            overlapping_with,
        });
    }

    let domain_clusters = analyze_domain_clusters(plugins);

    // Deduplicate ambiguity pairs (a-b is same as b-a)
    let mut seen = HashSet::new();
    let deduped_pairs: Vec<_> = ambiguity_pairs
        .into_iter()
        .filter(|pair| {
            let key = {
                let mut names = [pair.plugin_a.as_str(), pair.plugin_b.as_str()];
                names.sort();
                names.join("|")
            };
            seen.insert(key)
        })
        .collect();

    let high = results
        .iter()
        .filter(|r| r.confidence == Confidence::High)
        .count();
    let medium = results
        .iter()
        .filter(|r| r.confidence == Confidence::Medium)
        .count();
    let low = results
        .iter()
        .filter(|r| r.confidence == Confidence::Low)
        .count();

    let needs_author_review: Vec<_> = results
        .iter()
        .filter(|r| r.confidence == Confidence::Low || !r.overlapping_with.is_empty())
        .map(|r| r.name.clone())
        .collect();

    let always_useful: Vec<_> = plugins
        .iter()
        .filter(|p| p.is_always_useful)
        .map(|p| p.name.clone())
        .collect();

    // Estimate context savings:
    // Full skill dump: average ~8 lines × ~6 words × ~1.3 tokens = ~62 tokens per plugin
    // Brief index: average ~22 tokens per brief
    let full_tokens_per_plugin: f64 = 62.0;
    let brief_tokens_per_plugin: f64 = 22.0;
    let n = plugins.len() as f64;
    let full_total = n * full_tokens_per_plugin;
    let brief_total = n * brief_tokens_per_plugin;
    let savings_pct = ((full_total - brief_total) / full_total) * 100.0;

    let summary = Summary {
        total_plugins: plugins.len(),
        high_confidence: high,
        medium_confidence: medium,
        low_confidence: low,
        ambiguous_pairs: deduped_pairs.len(),
        always_useful_plugins: always_useful,
        needs_author_review,
        estimated_context_savings_pct: savings_pct.round(),
    };

    ValidationReport {
        plugins: results,
        ambiguity_pairs: deduped_pairs,
        domain_clusters,
        summary,
    }
}

fn main() -> anyhow::Result<()> {
    let plugins = build_plugins();
    let report = validate(&plugins);

    // Output JSON report
    let json = serde_json::to_string_pretty(&report)?;
    println!("{}", json);

    // Print human-readable summary to stderr
    eprintln!();
    eprintln!("══════════════════════════════════════════════════════════════");
    eprintln!("  SKILL BRIEF VALIDATION REPORT");
    eprintln!("══════════════════════════════════════════════════════════════");
    eprintln!();
    eprintln!("  Total plugins:     {}", report.summary.total_plugins);
    eprintln!(
        "  High confidence:   {} ({:.0}%)",
        report.summary.high_confidence,
        (report.summary.high_confidence as f64 / report.summary.total_plugins as f64) * 100.0
    );
    eprintln!("  Medium confidence: {}", report.summary.medium_confidence);
    eprintln!("  Low confidence:    {}", report.summary.low_confidence);
    eprintln!("  Ambiguous pairs:   {}", report.summary.ambiguous_pairs);
    eprintln!(
        "  Always-useful:     {:?}",
        report.summary.always_useful_plugins
    );
    eprintln!();
    eprintln!(
        "  Est. context savings: {:.0}% ({:.0} → {:.0} tokens)",
        report.summary.estimated_context_savings_pct,
        report.summary.total_plugins as f64 * 62.0,
        report.summary.total_plugins as f64 * 22.0,
    );
    eprintln!();
    eprintln!("  Domain clusters with collision risk:");
    for cluster in &report.domain_clusters {
        if cluster.plugins.len() >= 3 {
            eprintln!(
                "    {} ({}): {:?}",
                cluster.domain,
                cluster.plugins.len(),
                cluster.plugins
            );
        }
    }
    eprintln!();
    if !report.summary.needs_author_review.is_empty() {
        eprintln!(
            "  Needs author review: {:?}",
            report.summary.needs_author_review
        );
    }
    eprintln!();
    eprintln!("  Low confidence plugins:");
    for r in &report.plugins {
        if r.confidence == Confidence::Low {
            eprintln!("    {} — {}", r.name, r.notes.join("; "));
            if !r.overlapping_with.is_empty() {
                eprintln!("      overlaps with: {:?}", r.overlapping_with);
            }
        }
    }

    Ok(())
}
