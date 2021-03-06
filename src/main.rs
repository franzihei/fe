#![feature(external_doc)]
#![doc(include = "../README.md")]

use std::fs;
use std::io::{
    Error,
    Write,
};
use std::path::Path;

use clap::{
    arg_enum,
    values_t,
    App,
    Arg,
};

mod _utils;
use crate::_utils::pretty_curly_print;
use fe_compiler::types::CompiledModule;

const DEFAULT_OUTPUT_DIR_NAME: &str = "output";
const VERSION: &str = env!("CARGO_PKG_VERSION");

arg_enum! {
    #[derive(PartialEq, Debug)]
    pub enum CompilationTarget {
        Abi,
        Ast,
        Bytecode,
        Tokens,
        Yul,
    }
}

pub fn main() {
    let matches = App::new("Fe")
        .version(VERSION)
        .about("Compiler for the Fe language")
        .arg(
            Arg::with_name("input")
                .help("The input source file to use e.g erc20.fe")
                .index(1)
                .required(true),
        )
        .arg(
            Arg::with_name("output-dir")
                .short("o")
                .long("output-dir")
                .help("The directory to store the compiler output e.g /tmp/output")
                .takes_value(true)
                .default_value(DEFAULT_OUTPUT_DIR_NAME),
        )
        .arg(
            Arg::with_name("emit")
                .short("e")
                .long("emit")
                .help("Comma separated compile targets e.g. -e=bytecode,yul")
                .possible_values(&["abi", "bytecode", "ast", "tokens", "yul"])
                .default_value("abi,bytecode")
                .use_delimiter(true)
                .takes_value(true),
        )
        .arg(
            Arg::with_name("overwrite")
                .long("overwrite")
                .help("Overwrite contents of output directory`"),
        )
        .arg(
            Arg::with_name("optimize")
                .long("optimize")
                .help("Enables the Yul optimizer`"),
        )
        .get_matches();

    let input_file = matches.value_of("input").unwrap();
    let output_dir = matches.value_of("output-dir").unwrap();
    let overwrite = matches.is_present("overwrite");
    let optimize = matches.is_present("overwrite");
    let targets =
        values_t!(matches.values_of("emit"), CompilationTarget).unwrap_or_else(|e| e.exit());

    match compile_and_write(input_file, &targets, &output_dir, overwrite, optimize) {
        Ok(_) => println!("Compiled {}. Outputs in `{}`", input_file, output_dir),
        Err(err) => {
            println!("Unable to compile {}. \nError: {}", input_file, err);
            std::process::exit(1)
        }
    }
}

fn compile_and_write(
    src_file: &str,
    targets: &[CompilationTarget],
    output_dir: &str,
    overwrite: bool,
    optimize: bool,
) -> Result<(), String> {
    let src = fs::read_to_string(src_file).map_err(ioerr_to_string)?;
    let with_bytecode = targets.contains(&CompilationTarget::Bytecode);

    #[cfg(not(feature = "solc-backend"))]
    if with_bytecode {
        eprintln!("Warning: bytecode output requires 'solc-backend' feature. Try `cargo build --release --features solc-backend`. Skipping.");
    }

    let compiled_module =
        fe_compiler::compile(&src, with_bytecode, optimize).map_err(|error| error.to_string())?;

    write_compiled_module(compiled_module, targets, output_dir, overwrite)
}

fn write_compiled_module(
    mut module: CompiledModule,
    targets: &[CompilationTarget],
    output_dir: &str,
    overwrite: bool,
) -> Result<(), String> {
    let output_dir = Path::new(output_dir);
    if output_dir.is_file() {
        return Err(format!(
            "A file exists at path `{}`, the location of the output directory. Refusing to overwrite.",
            output_dir.display()
        ));
    }

    if !overwrite {
        verify_nonexistent_or_empty(output_dir)?;
    }

    fs::create_dir_all(output_dir).map_err(ioerr_to_string)?;

    if targets.contains(&CompilationTarget::Ast) {
        write_output(&output_dir.join("module.ast"), &module.fe_ast)?;
    }

    if targets.contains(&CompilationTarget::Tokens) {
        write_output(&output_dir.join("module.tokens"), &module.fe_tokens)?;
    }

    for (name, contract) in module.contracts.drain() {
        let contract_output_dir = output_dir.join(&name);
        fs::create_dir_all(&contract_output_dir).map_err(ioerr_to_string)?;

        if targets.contains(&CompilationTarget::Abi) {
            let file_name = format!("{}_abi.json", &name);
            write_output(&contract_output_dir.join(file_name), &contract.json_abi)?;
        }

        if targets.contains(&CompilationTarget::Yul) {
            let file_name = format!("{}_ir.yul", &name);
            write_output(
                &contract_output_dir.join(file_name),
                &pretty_curly_print(&contract.yul, 4),
            )?;
        }

        #[cfg(feature = "solc-backend")]
        if targets.contains(&CompilationTarget::Bytecode) {
            let file_name = format!("{}.bin", &name);
            write_output(&contract_output_dir.join(file_name), &contract.bytecode)?;
        }
    }

    Ok(())
}

fn write_output(path: &Path, content: &str) -> Result<(), String> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)
        .map_err(ioerr_to_string)?;
    file.write_all(content.as_bytes())
        .map_err(ioerr_to_string)?;
    Ok(())
}

fn ioerr_to_string(error: Error) -> String {
    format!("{}", error)
}

fn verify_nonexistent_or_empty(dir: &Path) -> Result<(), String> {
    if !dir.exists() || dir.read_dir().map_err(ioerr_to_string)?.next().is_none() {
        Ok(())
    } else {
        Err(format!(
            "Directory '{}' is not empty. Use --overwrite to overwrite.",
            dir.display()
        ))
    }
}
