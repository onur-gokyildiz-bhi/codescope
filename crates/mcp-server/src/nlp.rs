//! Natural Language Query Engine for the `ask` MCP tool.
//!
//! Parses natural language questions into structured intents + search terms.
//! Supports English and Turkish. Designed to be robust against:
//! - Greedy pattern matching (intent priority, not first-match)
//! - Multi-word search terms (not single-word fallback)
//! - Ambiguous phrasing (qualifier detection)

/// What the user wants to do
#[derive(Debug)]
pub enum Intent {
    /// "how many functions/classes/files?"
    Count(Entity),
    /// "list all functions" (no qualifier → unfiltered list)
    ListAll(Entity),
    /// "find functions related to X" (has qualifier/search terms)
    Search(Entity),
    /// "what calls X?", "call graph of X"
    CallGraph(CallDirection),
    /// "functions in main.rs"
    InFile,
    /// "largest/biggest functions"
    Largest,
    /// "show imports"
    Imports,
}

#[derive(Debug)]
pub enum Entity {
    Function,
    Class,
    File,
    Any,
}

#[derive(Debug)]
pub enum CallDirection {
    Callers,
    Callees,
    Both,
}

#[derive(Debug)]
pub struct ParsedQuestion {
    pub intent: Intent,
    pub search_terms: Vec<String>,
    pub file_path: Option<String>,
}

// ─── Keyword sets ────────────────────────────────────────────────────

fn is_count_keyword(w: &str) -> bool {
    matches!(
        w,
        "how" | "many" | "count" | "total" | "number" | "kaç" | "kac" | "tane" | "sayı" | "sayi"
    )
}

#[allow(dead_code)]
fn is_list_keyword(w: &str) -> bool {
    matches!(
        w,
        "list"
            | "show"
            | "display"
            | "all"
            | "every"
            | "listele"
            | "göster"
            | "goster"
            | "hepsi"
            | "tüm"
            | "tum"
    )
}

/// Keywords that indicate filtered search, NOT unfiltered listing.
/// When these appear, "list all functions" should become a search, not a dump.
fn is_qualifier_keyword(w: &str) -> bool {
    matches!(
        w,
        "related"
            | "about"
            | "matching"
            | "named"
            | "called"
            | "containing"
            | "like"
            | "similar"
            | "with"
            | "where"
            | "that"
            | "which"
            | "ilgili"
            | "adlı"
            | "adli"
            | "benzer"
            | "olan"
            | "içeren"
            | "iceren"
    )
}

fn is_caller_keyword(w: &str) -> bool {
    matches!(
        w,
        "caller" | "callers" | "calls" | "who" | "what" | "çağıran" | "cagiran" | "kim"
    )
}

fn is_callee_keyword(w: &str) -> bool {
    matches!(
        w,
        "callee" | "callees" | "called" | "çağırdığı" | "cagirdigi"
    )
}

#[allow(dead_code)]
fn is_callgraph_keyword(w: &str) -> bool {
    // "call graph", "calls", "callers", "callees", etc.
    w.starts_with("call") || is_caller_keyword(w) || is_callee_keyword(w)
}

fn is_size_keyword(w: &str) -> bool {
    matches!(
        w,
        "largest"
            | "biggest"
            | "longest"
            | "huge"
            | "complex"
            | "büyük"
            | "buyuk"
            | "uzun"
            | "karmaşık"
            | "karmasik"
    )
}

fn detect_entity(words: &[&str]) -> Entity {
    for w in words {
        match *w {
            "function" | "functions" | "func" | "funcs" | "fn" | "method" | "methods"
            | "fonksiyon" | "fonksiyonlar" | "fonksiyonları" | "fonksiyonlari" | "metod"
            | "metot" => return Entity::Function,
            "class" | "classes" | "struct" | "structs" | "type" | "types" | "interface"
            | "interfaces" | "trait" | "traits" | "enum" | "enums" | "sınıf" | "sinif"
            | "sınıflar" | "siniflar" => return Entity::Class,
            "file" | "files" | "dosya" | "dosyalar" => return Entity::File,
            _ => {}
        }
    }
    Entity::Any
}

/// Detect a file path in the question (e.g., "main.rs", "src/lib.rs")
fn detect_file_path(words: &[&str]) -> Option<String> {
    for w in words {
        let clean = w.trim_matches(|c: char| {
            !c.is_alphanumeric() && c != '/' && c != '\\' && c != '.' && c != '_' && c != '-'
        });
        // Must have a dot and look like a file path
        if clean.contains('.') && clean.len() > 2 {
            let has_ext = clean.ends_with(".rs")
                || clean.ends_with(".ts")
                || clean.ends_with(".tsx")
                || clean.ends_with(".js")
                || clean.ends_with(".jsx")
                || clean.ends_with(".py")
                || clean.ends_with(".go")
                || clean.ends_with(".java")
                || clean.ends_with(".cs")
                || clean.ends_with(".cpp")
                || clean.ends_with(".c")
                || clean.ends_with(".h")
                || clean.ends_with(".rb")
                || clean.ends_with(".md")
                || clean.ends_with(".toml")
                || clean.ends_with(".yaml")
                || clean.ends_with(".yml")
                || clean.ends_with(".json");
            let has_path_sep = clean.contains('/') || clean.contains('\\');
            if has_ext || has_path_sep {
                return Some(clean.to_string());
            }
        }
    }
    None
}

// ─── Stopwords (removed from search terms) ───────────────────────────

fn is_stopword(w: &str) -> bool {
    matches!(
        w,
        // English determiners/pronouns/prepositions
        "a" | "an" | "the" | "is" | "are" | "was" | "were" | "be" | "been" | "being"
        | "have" | "has" | "had" | "do" | "does" | "did"
        | "will" | "would" | "could" | "should" | "may" | "might" | "must" | "shall" | "can"
        | "i" | "me" | "my" | "you" | "your" | "we" | "our" | "they" | "their" | "it" | "its"
        | "he" | "she" | "him"
        | "this" | "that" | "these" | "those"
        | "in" | "on" | "at" | "to" | "for" | "of" | "with" | "by" | "from" | "about"
        | "and" | "or" | "but" | "nor" | "so" | "yet" | "not" | "no"
        // Action verbs (intent words, not search terms)
        | "find" | "show" | "list" | "get" | "give" | "tell" | "display" | "search"
        // Quantifiers
        | "all" | "every" | "each" | "any" | "some"
        // Question words
        | "what" | "where" | "which" | "who" | "whom" | "when" | "how" | "why"
        // Count words
        | "many" | "much" | "few" | "number" | "count" | "total"
        // Qualifier words (used for intent detection, not search)
        | "related" | "matching" | "named" | "called" | "containing" | "like" | "similar"
        // Entity type words (already captured by detect_entity)
        | "function" | "functions" | "func" | "funcs" | "fn" | "method" | "methods"
        | "class" | "classes" | "struct" | "structs" | "type" | "types"
        | "interface" | "interfaces" | "trait" | "traits" | "enum" | "enums"
        | "file" | "files"
        // Call graph words
        | "call" | "calls" | "caller" | "callers" | "callee" | "callees" | "graph"
        // Size words
        | "largest" | "biggest" | "longest"
        // Import words
        | "import" | "imports"
        // Turkish stopwords
        | "bir" | "bu" | "şu" | "su" | "o"
        | "ben" | "sen" | "biz" | "siz" | "onlar"
        | "ile" | "için" | "icin" | "dan" | "den" | "da" | "de"
        | "ve" | "veya" | "ama" | "fakat"
        | "ne" | "nedir" | "nerede" | "nasıl" | "nasil" | "hangi" | "kaç" | "kac" | "tane"
        | "bul" | "göster" | "goster" | "listele" | "ver" | "söyle" | "soyle"
        | "tüm" | "tum" | "hepsi" | "her" | "herhangi" | "hiç" | "hic"
        | "olan" | "ilgili" | "adlı" | "adli" | "benzer" | "içeren" | "iceren"
        | "bana" | "projede" | "dosya" | "dosyalar"
        | "fonksiyon" | "fonksiyonlar" | "fonksiyonları" | "fonksiyonlari"
        | "metod" | "metot" | "sınıf" | "sinif"
    )
}

// ─── Main parser ─────────────────────────────────────────────────────

pub fn parse_question(question: &str) -> ParsedQuestion {
    let words: Vec<&str> = question.split_whitespace().collect();

    let file_path = detect_file_path(&words);
    let entity = detect_entity(&words);

    let has_count = words.iter().any(|w| is_count_keyword(w))
        || question.contains("how many")
        || question.contains("kaç tane")
        || question.contains("kac tane");

    let has_qualifier = words.iter().any(|w| is_qualifier_keyword(w));

    let has_size = words.iter().any(|w| is_size_keyword(w))
        || question.contains("en büyük")
        || question.contains("en buyuk");

    let has_import = words.iter().any(|w| matches!(*w, "import" | "imports"));

    // Call graph detection — check for "call graph", "who/what calls", "callers of"
    let has_callgraph = question.contains("call graph")
        || question.contains("call tree")
        || (words.iter().any(|w| is_caller_keyword(w))
            && words
                .iter()
                .any(|w| matches!(*w, "calls" | "call" | "çağırıyor" | "cagiriyor")))
        || (words.iter().any(|w| matches!(*w, "callers" | "callees"))
            && words.iter().any(|w| matches!(*w, "of" | "for")));

    // Determine call direction
    let call_direction = if has_callgraph {
        if question.contains("call graph") || question.contains("call tree") {
            Some(CallDirection::Both)
        } else if words.iter().any(|w| is_callee_keyword(w))
            || question.contains("what does")
            || question.contains("ne çağırıyor")
            || question.contains("ne cagiriyor")
        {
            Some(CallDirection::Callees)
        } else {
            // "who calls", "callers of", "what calls" → callers
            Some(CallDirection::Callers)
        }
    } else {
        None
    };

    // ── Intent classification (priority order) ──
    // Priority matters! More specific intents must be checked first.

    let intent = if has_callgraph {
        Intent::CallGraph(call_direction.unwrap_or(CallDirection::Both))
    } else if has_count {
        Intent::Count(entity)
    } else if has_size {
        Intent::Largest
    } else if has_import && !has_qualifier {
        Intent::Imports
    } else if file_path.is_some() && !has_qualifier {
        // "functions in main.rs" → InFile
        // BUT "functions in main.rs related to parsing" → Search
        Intent::InFile
    } else if has_qualifier {
        // Any qualifier keyword → Search (even if "all" is present)
        Intent::Search(entity)
    } else {
        // No qualifier: check if there are search terms after removing stopwords
        let terms = extract_terms(&words, &file_path);
        if terms.is_empty() {
            Intent::ListAll(entity)
        } else {
            Intent::Search(entity)
        }
    };

    // Extract search terms
    let search_terms = extract_terms(&words, &file_path);

    ParsedQuestion {
        intent,
        search_terms,
        file_path,
    }
}

/// Extract meaningful search terms from the question.
/// Removes stopwords, intent keywords, entity type words.
/// Preserves snake_case and camelCase identifiers.
fn extract_terms(words: &[&str], file_path: &Option<String>) -> Vec<String> {
    let mut terms = Vec::new();
    let file_path_lower = file_path.as_ref().map(|p| p.to_lowercase());

    for w in words {
        let clean = w.trim_matches(|c: char| !c.is_alphanumeric() && c != '_' && c != '-');
        if clean.is_empty() || clean.len() < 2 {
            continue;
        }
        // Skip if this word IS the file path
        if let Some(ref fp) = file_path_lower {
            if clean == fp.as_str() {
                continue;
            }
        }
        // Skip stopwords
        if is_stopword(clean) {
            continue;
        }
        // Skip pure numbers
        if clean.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        terms.push(clean.to_string());
    }

    // Deduplicate while preserving order
    let mut seen = std::collections::HashSet::new();
    terms.retain(|t| seen.insert(t.clone()));

    terms
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_search_with_qualifier() {
        let p = parse_question("find all functions related to codebook quantize or dequantize");
        assert!(matches!(p.intent, Intent::Search(Entity::Function)));
        assert!(p.search_terms.contains(&"codebook".to_string()));
        assert!(p.search_terms.contains(&"quantize".to_string()));
        assert!(p.search_terms.contains(&"dequantize".to_string()));
    }

    #[test]
    fn test_list_all_no_qualifier() {
        let p = parse_question("list all functions");
        assert!(matches!(p.intent, Intent::ListAll(Entity::Function)));
    }

    #[test]
    fn test_count() {
        let p = parse_question("how many classes are there?");
        assert!(matches!(p.intent, Intent::Count(Entity::Class)));
    }

    #[test]
    fn test_count_turkish() {
        let p = parse_question("kaç tane fonksiyon var?");
        assert!(matches!(p.intent, Intent::Count(Entity::Function)));
    }

    #[test]
    fn test_callers() {
        let p = parse_question("what calls parse_file?");
        assert!(matches!(
            p.intent,
            Intent::CallGraph(CallDirection::Callers)
        ));
        assert!(p.search_terms.contains(&"parse_file".to_string()));
    }

    #[test]
    fn test_call_graph() {
        let p = parse_question("show call graph for embed_functions");
        assert!(matches!(p.intent, Intent::CallGraph(CallDirection::Both)));
        assert!(p.search_terms.contains(&"embed_functions".to_string()));
    }

    #[test]
    fn test_in_file() {
        let p = parse_question("functions in src/main.rs");
        assert!(matches!(p.intent, Intent::InFile));
        assert_eq!(p.file_path.as_deref(), Some("src/main.rs"));
    }

    #[test]
    fn test_largest() {
        let p = parse_question("show me the largest functions");
        assert!(matches!(p.intent, Intent::Largest));
    }

    #[test]
    fn test_search_with_identifier() {
        let p = parse_question("find binary_quantize");
        assert!(matches!(p.intent, Intent::Search(_)));
        assert!(p.search_terms.contains(&"binary_quantize".to_string()));
    }

    #[test]
    fn test_search_multiword() {
        let p = parse_question("functions about error handling");
        assert!(matches!(p.intent, Intent::Search(Entity::Function)));
        assert!(p.search_terms.contains(&"error".to_string()));
        assert!(p.search_terms.contains(&"handling".to_string()));
    }

    #[test]
    fn test_ts_not_greedy_match() {
        // Previously ".ts" anywhere would match the "in file" branch
        let p = parse_question("what functions handle ts transpilation");
        assert!(matches!(p.intent, Intent::Search(_)));
        assert!(!matches!(p.intent, Intent::InFile));
    }

    #[test]
    fn test_callers_of() {
        let p = parse_question("callers of embed_functions");
        assert!(matches!(
            p.intent,
            Intent::CallGraph(CallDirection::Callers)
        ));
        assert!(p.search_terms.contains(&"embed_functions".to_string()));
    }
}
