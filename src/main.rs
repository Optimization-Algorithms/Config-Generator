use std::fs::{File, create_dir_all};
use std::io::Write;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use structopt::StructOpt;

use sailfish::TemplateOnce;

struct ConfigIterator {
    count: usize,
    params: Vec<(String, bool)>,
}

impl ConfigIterator {
    fn new(params: Vec<String>) -> Self {
        let count = 1 << params.len();
        let params = params.into_iter().map(|p| (p, false)).collect();
        Self { count, params }
    }

    fn has_next(&mut self) -> bool {
        self.count > 0
    }

    fn compute_next(&mut self) {
        self.count -= 1;
        let mut prev: Option<bool> = None;
        for (_, bool_val) in self.params.iter_mut() {
            if let Some(true) = prev {
                continue;
            }
            *bool_val = !(*bool_val);
            prev = Some(*bool_val);
        }
    }
}

struct KernelSearchConfigFactory {
    iter: ConfigIterator,
    constant: Vec<(String, bool)>,
}

impl KernelSearchConfigFactory {
    fn new(test_param: Vec<String>, constant_param: Vec<(String, bool)>) -> Self {
        Self {
            iter: ConfigIterator::new(test_param),
            constant: constant_param,
        }
    }

    fn build_next(&self) -> String {
        let constant = Self::collect_parameters(self.constant.iter());
        let variable = Self::collect_parameters(self.iter.params.iter());
        format!(
            "# Constant Parameters\n{}# Test Parameters\n{}",
            constant, variable
        )
    }

    fn collect_parameters<'a>(params: impl Iterator<Item = &'a (String, bool)>) -> String {
        params
            .map(|(param, value)| format!("{}: {}\n", param, value))
            .fold(String::new(), |acc, next| acc + &next)
    }
}

impl Iterator for KernelSearchConfigFactory {
    type Item = String;
    fn next(&mut self) -> Option<Self::Item> {
        if self.iter.has_next() {
            let output = self.build_next();
            self.iter.compute_next();
            Some(output)
        } else {
            None
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
enum TestParam {
    Constant(bool),
    Variable,
}

#[derive(StructOpt, Debug)]
struct CLIArgs {
    #[structopt(help="Set instance name")]
    instance: String,
    #[structopt(help="Set config file")]
    config: PathBuf,
    #[structopt(short = "o", long = "output-dir", help="Specify output directory")]
    output_dir: Option<PathBuf>,
}

#[derive(Serialize, Deserialize, Debug)]
struct Config {
    iterations: usize,
    bucket_count: usize,
    time_limit: usize,

    parameters: Vec<(String, TestParam)>,
}

impl<'a> Config {
    fn build_inner_data(
        self,
        base_name: &'a str,
    ) -> (OutputConfig<'a>, Vec<String>, Vec<(String, bool)>) {
        let output_config = self.build_base_template(base_name);
        let (test_params, const_param) = Self::convert_to_param_list(self.parameters);
        (output_config, test_params, const_param)
    }

    fn build_base_template(&self, base_name: &'a str) -> OutputConfig<'a> {
        OutputConfig {
            iterations: self.iterations,
            bucket_count: self.bucket_count,
            time_limit: self.time_limit,
            base_name,
        }
    }

    fn convert_to_param_list(
        parameters: Vec<(String, TestParam)>,
    ) -> (Vec<String>, Vec<(String, bool)>) {
        parameters
            .into_iter()
            .fold((vec![], vec![]), |(mut var, mut cons), (name, kind)| {
                match kind {
                    TestParam::Constant(value) => cons.push((name, value)),
                    TestParam::Variable => var.push(name),
                }
                (var, cons)
            })
    }
}

struct OutputConfig<'a> {
    iterations: usize,
    bucket_count: usize,
    time_limit: usize,
    base_name: &'a str,
}

#[derive(TemplateOnce)]
#[template(path = "config_template.stpl")]
struct OutputTemplate<'a> {
    index: usize,
    config: &'a OutputConfig<'a>,
    body: String,
}

impl<'a> OutputTemplate<'a> {
    fn new(index: usize, config: &'a OutputConfig, body: String) -> Self {
        Self {
            index,
            config,
            body,
        }
    }
}

fn load_config(config_file: &PathBuf) -> Result<Config, Box<dyn std::error::Error>> {
    let file = File::open(config_file)?;
    let config = serde_yaml::from_reader(file)?;
    Ok(config)
}

fn output_config(conf: OutputTemplate, name: &str, index: usize, target_dir: &Option<PathBuf>) -> std::io::Result<()> {
    let file_name = format!("{}-{}.yml", name, index);
    let file_path = if let Some(target_dir) = target_dir {
        target_dir.join(file_name)
    } else {
        PathBuf::from(file_name)
    };
    let mut output_file = File::create(file_path)?;
    let config = conf.render_once().unwrap();
    output_file.write(config.as_bytes())?;
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = CLIArgs::from_args();
    let config = load_config(&args.config)?;
    let (base_config, test_param, const_param) = config.build_inner_data(&args.instance);

    if let Some(output_dir) = &args.output_dir {
        if !output_dir.is_dir() {
            create_dir_all(output_dir)?;
        }
    }

    for (i, conf) in KernelSearchConfigFactory::new(test_param, const_param)
        .enumerate()
        .map(|(i, conf)| (i, OutputTemplate::new(i, &base_config, conf)))
    {
        output_config(conf, &args.instance, i, &args.output_dir)?
    }

    Ok(())
}
