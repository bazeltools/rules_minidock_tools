use anyhow::bail;
use clap::Parser;
use std::{path::PathBuf, io::Write};

#[derive(Parser, Debug)]
#[clap(name = "basic")]
struct Opt {
    #[clap(long, parse(from_os_str))]
    pusher_config_path: PathBuf,

    #[clap(long, parse(from_os_str))]
    relative_search_path: PathBuf,


    #[clap(long, parse(from_os_str))]
    config_output: PathBuf,

    #[clap(long, parse(from_os_str))]
    directory_output: PathBuf,

    #[clap(long, parse(from_os_str))]
    pusher_path: PathBuf,

    #[clap(long, parse(from_os_str))]
    launcher_path: PathBuf,


}

static PRELUDE: & str = r#"
#!/usr/bin/env bash
# Copyright 2017 The Bazel Authors. All rights reserved.
#
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
#    http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.

set -eu

function guess_runfiles() {
    if [ -d ${BASH_SOURCE[0]}.runfiles ]; then
        # Runfiles are adjacent to the current script.
        echo "$( cd ${BASH_SOURCE[0]}.runfiles && pwd )"
    else
        # The current script is within some other script's runfiles.
        mydir="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
        echo $mydir | sed -e 's|\(.*\.runfiles\)/.*|\1|'
    fi
}

RUNFILES="${PYTHON_RUNFILES:-$(guess_runfiles)}"
"#;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let opt = Opt::parse();

    let pusher_config = rules_minidock_tools::docker_types::pusher_config::Layers::parse_file(
        &opt.pusher_config_path,
    )?;

    let relative_search_path = opt.relative_search_path.clone();

    let (merge_config, layers) = rules_minidock_tools::merge_outputs::merge(&pusher_config, &relative_search_path).await?;

    let config_path = opt.directory_output.join("config.json");
    merge_config.write_file(&config_path)?;

    let mut args = Vec::default();

    for (idx, output_layer) in layers.layers.iter().enumerate() {

        use std::fs::File;
        use std::io::BufWriter;

        let p = opt.directory_output.join(format!("{}.sha256", idx));
        let file = File::create(&p)?;

        let mut writer = BufWriter::new(file);
        writer.write(output_layer.sha256.to_string().as_bytes())?;

        args.push(format!("--layer=$RUNFILES/{},{}", output_layer.content, p.to_string_lossy()))

     }
    //  --manifest=${RUNFILES}/io_bazel_rules_docker/test_simple_imagen.0.manifest

    let launcher_content = format!(r#"${{RUNFILES}}/{pusher_path} \
    --config=${{RUNFILES}}/{config_path} \
    {layers} \
    --format=Docker \
    --dst=registry.us-west-2.streamingtest.titus.netflix.net:7002/ae/ianoc_tests_docker/test:unchanged_tag1 \
    -skip-unchanged-digest
    "#, pusher_path = &opt.pusher_path.to_string_lossy(), config_path =config_path.to_string_lossy(),
    layers = args.join("\n"));


    use std::fs::File;
    use std::io::BufWriter;

    // Open the file in read-only mode with buffer.
    let file = File::create(&opt.launcher_path)?;
    let mut writer = BufWriter::new(file);
    writer.write(launcher_content.as_bytes())?;

    println!("merged_config: {:#?}", merge_config);
    println!("layers: {:#?}", layers);
    Ok(())
}
