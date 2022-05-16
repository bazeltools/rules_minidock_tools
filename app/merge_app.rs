use anyhow::{bail, Error};
use clap::Parser;
use rules_minidock_tools::{hash::sha256_value::Sha256Value, docker_types::PathPair, merge_outputs::OutputLayer};
use std::{path::{PathBuf, Path}, io::{Write, BufWriter}, fs::File};

#[derive(Parser, Debug)]
#[clap(name = "basic")]
struct Opt {
    #[clap(long, parse(from_os_str))]
    pusher_config_path: PathBuf,

    #[clap(long, parse(from_os_str))]
    relative_search_path: Option<PathBuf>,

    #[clap(long, parse(from_os_str))]
    directory_output: PathBuf,

    #[clap(long, parse(from_os_str))]
    directory_output_short_path: PathBuf,

}


async fn write_sha<P: AsRef<Path>>(path: P, sha_value: &Sha256Value) -> Result<(), Error> {
    let file = File::create(path.as_ref())?;
    let mut writer = BufWriter::new(file);
    writer.write_all(sha_value.to_string().as_bytes())?;
    Ok(())
}

struct RetF {
    outer_sha: PathPair,
    inner_sha: PathPair
}

async fn emit_shas<P: AsRef<Path>, Q: AsRef<Path>>(directory_output: P, directory_output_short: Q, idx: usize, output_layer: &OutputLayer) -> Result<RetF, Error> {
    let outer_nme = format!("{}.outer.sha256", idx);
    let inner_nme = format!("{}.inner.sha256", idx);

    write_sha(
        directory_output.as_ref().join(&outer_nme),
        &output_layer.sha256
    ).await?;

    write_sha(
        directory_output.as_ref().join(&inner_nme),
        &output_layer.inner_sha_v
    ).await?;

    Ok(
        RetF {
            outer_sha: PathPair { short_path: directory_output_short.as_ref().join(&outer_nme).to_string_lossy().to_string(), path: directory_output.as_ref().join(&outer_nme).to_string_lossy().to_string() },
            inner_sha: PathPair { short_path: directory_output_short.as_ref().join(&inner_nme).to_string_lossy().to_string(), path: directory_output.as_ref().join(&inner_nme).to_string_lossy().to_string() },
        }
    )
}


#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let opt = Opt::parse();

    let pusher_config = rules_minidock_tools::docker_types::pusher_config::Layers::parse_file(
        &opt.pusher_config_path,
    )?;

    let relative_search_path = opt.relative_search_path.clone();

    let (merge_config, mut manifest, layers) = rules_minidock_tools::merge_outputs::merge(&pusher_config, &relative_search_path).await?;

    let config_path = opt.directory_output.join("config.json");
    merge_config.write_file(&config_path)?;

    let (config_sha, config_len) = Sha256Value::from_path(&config_path).await?;

    manifest.update_config(config_sha, config_len);


    let manifest_path = opt.directory_output.join("manifest.json");
    manifest.write_file(&manifest_path)?;


    let mut args = Vec::default();

    for (idx, output_layer) in layers.layers.iter().enumerate() {
        let ret_f = emit_shas(
            &opt.directory_output,
            &opt.directory_output_short_path,
            idx,
            output_layer
        ).await?;

        args.push(format!("|ARGS=\"$ARGS --layer={},{},{}\"", &output_layer.content.short_path, ret_f.outer_sha.short_path, ret_f.inner_sha.short_path))
     }

    let include_data = format!(r#"
        |
        |cat {directory_output_short_path}/config.json
        |pwd
        |ARGS=""
        |ARGS="$ARGS --config={directory_output_short_path}/config.json" \
        {layers}
    "#, directory_output_short_path = opt.directory_output_short_path.to_string_lossy(),
    layers = args.join("\n"));


    use std::fs::File;
    use std::io::BufWriter;

    let trimmed_content = include_data.lines().map(|ln| {
        if let Some(off) = ln.find('|') {
            if off == ln.len() {
                ""
            } else {
                ln.split_at(off+1).1
            }
        } else {
            ln
        }
    }).collect::<Vec<&str>>().join("\n");
    // Open the file in read-only mode with buffer.
    let file = File::create(opt.directory_output.join("launcher_helper"))?;
    let mut writer = BufWriter::new(file);
    writer.write_all(trimmed_content.as_bytes())?;

    println!("merged_config: {:#?}", merge_config);
    println!("layers: {:#?}", layers);
    Ok(())
}
