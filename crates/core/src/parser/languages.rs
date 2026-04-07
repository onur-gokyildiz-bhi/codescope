use tree_sitter::Language;

pub struct LanguageConfig {
    pub name: String,
    pub language: Language,
    pub extensions: Vec<String>,
}

pub struct LanguageRegistry {
    languages: Vec<LanguageConfig>,
}

impl Default for LanguageRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl LanguageRegistry {
    pub fn new() -> Self {
        let languages = vec![
            // TypeScript
            LanguageConfig {
                name: "typescript".into(),
                language: tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
                extensions: vec!["ts".into()],
            },
            // TSX
            LanguageConfig {
                name: "tsx".into(),
                language: tree_sitter_typescript::LANGUAGE_TSX.into(),
                extensions: vec!["tsx".into()],
            },
            // JavaScript
            LanguageConfig {
                name: "javascript".into(),
                language: tree_sitter_javascript::LANGUAGE.into(),
                extensions: vec!["js".into(), "jsx".into(), "mjs".into(), "cjs".into()],
            },
            // Python
            LanguageConfig {
                name: "python".into(),
                language: tree_sitter_python::LANGUAGE.into(),
                extensions: vec!["py".into(), "pyi".into()],
            },
            // Rust
            LanguageConfig {
                name: "rust".into(),
                language: tree_sitter_rust::LANGUAGE.into(),
                extensions: vec!["rs".into()],
            },
            // Go
            LanguageConfig {
                name: "go".into(),
                language: tree_sitter_go::LANGUAGE.into(),
                extensions: vec!["go".into()],
            },
            // Java
            LanguageConfig {
                name: "java".into(),
                language: tree_sitter_java::LANGUAGE.into(),
                extensions: vec!["java".into()],
            },
            // C
            LanguageConfig {
                name: "c".into(),
                language: tree_sitter_c::LANGUAGE.into(),
                extensions: vec!["c".into(), "h".into()],
            },
            // C++
            LanguageConfig {
                name: "cpp".into(),
                language: tree_sitter_cpp::LANGUAGE.into(),
                extensions: vec![
                    "cpp".into(),
                    "cc".into(),
                    "cxx".into(),
                    "hpp".into(),
                    "hh".into(),
                    "hxx".into(),
                ],
            },
            // C#
            LanguageConfig {
                name: "csharp".into(),
                language: tree_sitter_c_sharp::LANGUAGE.into(),
                extensions: vec!["cs".into()],
            },
            // Ruby
            LanguageConfig {
                name: "ruby".into(),
                language: tree_sitter_ruby::LANGUAGE.into(),
                extensions: vec!["rb".into(), "rake".into()],
            },
            // PHP
            LanguageConfig {
                name: "php".into(),
                language: tree_sitter_php::LANGUAGE_PHP.into(),
                extensions: vec!["php".into()],
            },
            // Swift
            LanguageConfig {
                name: "swift".into(),
                language: tree_sitter_swift::LANGUAGE.into(),
                extensions: vec!["swift".into()],
            },
            // Dart
            LanguageConfig {
                name: "dart".into(),
                language: tree_sitter_dart::LANGUAGE.into(),
                extensions: vec!["dart".into()],
            },
            // Scala
            LanguageConfig {
                name: "scala".into(),
                language: tree_sitter_scala::LANGUAGE.into(),
                extensions: vec!["scala".into(), "sc".into()],
            },
            // Lua
            LanguageConfig {
                name: "lua".into(),
                language: tree_sitter_lua::LANGUAGE.into(),
                extensions: vec!["lua".into()],
            },
            // Zig
            LanguageConfig {
                name: "zig".into(),
                language: tree_sitter_zig::LANGUAGE.into(),
                extensions: vec!["zig".into()],
            },
            // Elixir
            LanguageConfig {
                name: "elixir".into(),
                language: tree_sitter_elixir::LANGUAGE.into(),
                extensions: vec!["ex".into(), "exs".into()],
            },
            // Haskell
            LanguageConfig {
                name: "haskell".into(),
                language: tree_sitter_haskell::LANGUAGE.into(),
                extensions: vec!["hs".into()],
            },
            // OCaml
            LanguageConfig {
                name: "ocaml".into(),
                language: tree_sitter_ocaml::LANGUAGE_OCAML.into(),
                extensions: vec!["ml".into(), "mli".into()],
            },
            // HTML
            LanguageConfig {
                name: "html".into(),
                language: tree_sitter_html::LANGUAGE.into(),
                extensions: vec!["html".into(), "htm".into()],
            },
            // Julia
            LanguageConfig {
                name: "julia".into(),
                language: tree_sitter_julia::LANGUAGE.into(),
                extensions: vec!["jl".into()],
            },
            // Bash / Shell
            LanguageConfig {
                name: "bash".into(),
                language: tree_sitter_bash::LANGUAGE.into(),
                extensions: vec!["sh".into(), "bash".into(), "zsh".into()],
            },
            // R
            LanguageConfig {
                name: "r".into(),
                language: tree_sitter_r::LANGUAGE.into(),
                extensions: vec!["r".into(), "R".into()],
            },
            // CSS
            LanguageConfig {
                name: "css".into(),
                language: tree_sitter_css::LANGUAGE.into(),
                extensions: vec!["css".into()],
            },
            // Erlang
            LanguageConfig {
                name: "erlang".into(),
                language: tree_sitter_erlang::LANGUAGE.into(),
                extensions: vec!["erl".into(), "hrl".into()],
            },
            // Objective-C
            LanguageConfig {
                name: "objc".into(),
                language: tree_sitter_objc::LANGUAGE.into(),
                extensions: vec!["m".into(), "mm".into()],
            },
            // HCL / Terraform
            LanguageConfig {
                name: "hcl".into(),
                language: tree_sitter_hcl::LANGUAGE.into(),
                extensions: vec!["hcl".into(), "tf".into(), "tfvars".into()],
            },
            // Nix
            LanguageConfig {
                name: "nix".into(),
                language: tree_sitter_nix::LANGUAGE.into(),
                extensions: vec!["nix".into()],
            },
            // CMake
            LanguageConfig {
                name: "cmake".into(),
                language: tree_sitter_cmake::LANGUAGE.into(),
                extensions: vec!["cmake".into()],
            },
            // Makefile
            LanguageConfig {
                name: "make".into(),
                language: tree_sitter_make::LANGUAGE.into(),
                extensions: vec!["mk".into()],
            },
            // Verilog / SystemVerilog
            LanguageConfig {
                name: "verilog".into(),
                language: tree_sitter_verilog::LANGUAGE.into(),
                extensions: vec!["v".into(), "sv".into(), "svh".into()],
            },
            // Fortran
            LanguageConfig {
                name: "fortran".into(),
                language: tree_sitter_fortran::LANGUAGE.into(),
                extensions: vec![
                    "f".into(),
                    "f90".into(),
                    "f95".into(),
                    "f03".into(),
                    "f08".into(),
                ],
            },
            // GLSL
            LanguageConfig {
                name: "glsl".into(),
                language: tree_sitter_glsl::LANGUAGE_GLSL.into(),
                extensions: vec!["glsl".into(), "vert".into(), "frag".into(), "comp".into()],
            },
            // GraphQL
            LanguageConfig {
                name: "graphql".into(),
                language: tree_sitter_graphql::LANGUAGE.into(),
                extensions: vec!["graphql".into(), "gql".into()],
            },
        ];

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
