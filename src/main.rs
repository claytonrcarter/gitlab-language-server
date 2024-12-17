mod lsp;

#[tokio::main]
async fn main() {
    match
        std::env::args().nth(1)
        // env::args().nth(2),
        // env::args().nth(3),
        // env::args().nth(4),
     {
        // (Some(arg), Some(file), Some(line), Some(column)) if arg == "--debug" => {
        //     #[allow(clippy::unwrap_used)]
        //     let (file, source) = {
        //         let file = Path::new(&file).canonicalize().unwrap();
        //         let file = file.as_os_str().to_str().unwrap().to_string();
        //         let source = contents_of_path(&file).unwrap();
        //         (file, source)
        //     };

        //     let mut be = LedgerBackend::new();
        //     be.parse_document(&source);

        //     let mut visited = HashSet::new();
        //     #[allow(clippy::unwrap_used)]
        //     let completions = be
        //         .completions_for_position(
        //             &file,
        //             &source,
        //             &tower_lsp::lsp_types::Position {
        //                 line: line.parse().unwrap(),
        //                 character: column.parse().unwrap(),
        //             },
        //             &mut visited,
        //         )
        //         .unwrap();
        //     println!("completions:\n{completions:?}");

        //     return;
        // }
        Some(arg) if arg == "lsp" => lsp::run_server().await,
        _ => {
            eprintln!("Usage: gitlab-language-server lsp => run the LSP server using stdin/stdout");
        }
    }
}
