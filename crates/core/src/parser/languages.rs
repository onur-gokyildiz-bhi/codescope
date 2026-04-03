use tree_sitter::Language;

pub struct LanguageConfig {
    pub name: String,
    pub language: Language,
    pub extensions: Vec<String>,
}

pub struct LanguageRegistry {
    languages: Vec<LanguageConfig>,
}

impl LanguageRegistry {
    pub fn new() -> Self {
        let mut languages = Vec::new();

        // TypeScript
        languages.push(LanguageConfig {
            name: "typescript".into(),
            language: tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            extensions: vec!["ts".into()],
        });

        // TSX
        languages.push(LanguageConfig {
            name: "tsx".into(),
            language: tree_sitter_typescript::LANGUAGE_TSX.into(),
            extensions: vec!["tsx".into()],
        });

        // JavaScript
        languages.push(LanguageConfig {
            name: "javascript".into(),
            language: tree_sitter_javascript::LANGUAGE.into(),
            extensions: vec!["js".into(), "jsx".into(), "mjs".into(), "cjs".into()],
        });

        // Python
        languages.push(LanguageConfig {
            name: "python".into(),
            language: tree_sitter_python::LANGUAGE.into(),
            extensions: vec!["py".into(), "pyi".into()],
        });

        // Rust
        languages.push(LanguageConfig {
            name: "rust".into(),
            language: tree_sitter_rust::LANGUAGE.into(),
            extensions: vec!["rs".into()],
        });

        // Go
        languages.push(LanguageConfig {
            name: "go".into(),
            language: tree_sitter_go::LANGUAGE.into(),
            extensions: vec!["go".into()],
        });

        // Java
        languages.push(LanguageConfig {
            name: "java".into(),
            language: tree_sitter_java::LANGUAGE.into(),
            extensions: vec!["java".into()],
        });

        // C
        languages.push(LanguageConfig {
            name: "c".into(),
            language: tree_sitter_c::LANGUAGE.into(),
            extensions: vec!["c".into(), "h".into()],
        });

        // C++
        languages.push(LanguageConfig {
            name: "cpp".into(),
            language: tree_sitter_cpp::LANGUAGE.into(),
            extensions: vec!["cpp".into(), "cc".into(), "cxx".into(), "hpp".into(), "hh".into(), "hxx".into()],
        });

        // C#
        languages.push(LanguageConfig {
            name: "csharp".into(),
            language: tree_sitter_c_sharp::LANGUAGE.into(),
            extensions: vec!["cs".into()],
        });

        // Ruby
        languages.push(LanguageConfig {
            name: "ruby".into(),
            language: tree_sitter_ruby::LANGUAGE.into(),
            extensions: vec!["rb".into(), "rake".into()],
        });

        // PHP
        languages.push(LanguageConfig {
            name: "php".into(),
            language: tree_sitter_php::LANGUAGE_PHP.into(),
            extensions: vec!["php".into()],
        });

        // Swift
        languages.push(LanguageConfig {
            name: "swift".into(),
            language: tree_sitter_swift::LANGUAGE.into(),
            extensions: vec!["swift".into()],
        });

        // Dart
        languages.push(LanguageConfig {
            name: "dart".into(),
            language: tree_sitter_dart::LANGUAGE.into(),
            extensions: vec!["dart".into()],
        });

        // Scala
        languages.push(LanguageConfig {
            name: "scala".into(),
            language: tree_sitter_scala::LANGUAGE.into(),
            extensions: vec!["scala".into(), "sc".into()],
        });

        // Lua
        languages.push(LanguageConfig {
            name: "lua".into(),
            language: tree_sitter_lua::LANGUAGE.into(),
            extensions: vec!["lua".into()],
        });

        // Zig
        languages.push(LanguageConfig {
            name: "zig".into(),
            language: tree_sitter_zig::LANGUAGE.into(),
            extensions: vec!["zig".into()],
        });

        // Elixir
        languages.push(LanguageConfig {
            name: "elixir".into(),
            language: tree_sitter_elixir::LANGUAGE.into(),
            extensions: vec!["ex".into(), "exs".into()],
        });

        // Haskell
        languages.push(LanguageConfig {
            name: "haskell".into(),
            language: tree_sitter_haskell::LANGUAGE.into(),
            extensions: vec!["hs".into()],
        });

        // OCaml
        languages.push(LanguageConfig {
            name: "ocaml".into(),
            language: tree_sitter_ocaml::LANGUAGE_OCAML.into(),
            extensions: vec!["ml".into(), "mli".into()],
        });

        // HTML
        languages.push(LanguageConfig {
            name: "html".into(),
            language: tree_sitter_html::LANGUAGE.into(),
            extensions: vec!["html".into(), "htm".into()],
        });

        // Julia
        languages.push(LanguageConfig {
            name: "julia".into(),
            language: tree_sitter_julia::LANGUAGE.into(),
            extensions: vec!["jl".into()],
        });

        Self { languages }
    }

    pub fn get_by_extension(&self, ext: &str) -> Option<&LanguageConfig> {
        self.languages
            .iter()
            .find(|l| l.extensions.iter().any(|e| e == ext))
    }

    pub fn language_names(&self) -> Vec<String> {
        self.languages.iter().map(|l| l.name.clone()).collect()
    }
}
