use std::io;

use clap_complete::Shell;
use clap_complete_nushell::Nushell;

use crate::cli::Cli;
use crate::cli::Generate;

const MAN_TEMPLATE: &str = include_str!("../doc/man-template.roff");

pub fn generate(bin_name: &str, generate: Generate) {
    let mut app = Cli::into_app();

    match generate {
        Generate::CompleteBash => {
            clap_complete::generate(Shell::Bash, &mut app, bin_name, &mut io::stdout());
        }
        Generate::CompleteElvish => {
            clap_complete::generate(Shell::Elvish, &mut app, bin_name, &mut io::stdout());
        }
        Generate::CompleteFish => {
            clap_complete::generate(Shell::Fish, &mut app, bin_name, &mut io::stdout());
        }
        Generate::CompleteNushell => {
            clap_complete::generate(Nushell, &mut app, bin_name, &mut io::stdout());
        }
        Generate::CompletePowershell => {
            clap_complete::generate(Shell::PowerShell, &mut app, bin_name, &mut io::stdout());
        }
        Generate::CompleteZsh => {
            clap_complete::generate(Shell::Zsh, &mut app, bin_name, &mut io::stdout());
        }
        Generate::Man => {
            generate_manpages(&mut app);
        }
    }
}

fn generate_manpages(app: &mut clap::Command) {
    use roff::{bold, italic, roman, Roff};
    use time::OffsetDateTime as DateTime;

    let items: Vec<_> = app.get_arguments().filter(|i| !i.is_hide_set()).collect();

    let mut request_items_roff = Roff::new();
    let request_items = items
        .iter()
        .find(|opt| opt.get_id() == "raw_rest_args")
        .unwrap();
    let request_items_help = request_items
        .get_long_help()
        .or_else(|| request_items.get_help())
        .expect("request_items is missing help")
        .to_string();

    // replace the indents in request_item help with proper roff controls
    // For example:
    //
    // ```
    // normal help normal help
    // normal help normal help
    //
    //   request-item-1
    //     help help
    //
    //   request-item-2
    //     help help
    //
    // normal help normal help
    // ```
    //
    // Should look like this with roff controls
    //
    // ```
    // normal help normal help
    // normal help normal help
    // .RS 12
    // .TP
    // request-item-1
    // help help
    // .TP
    // request-item-2
    // help help
    // .RE
    //
    // .RS
    // normal help normal help
    // .RE
    // ```
    let lines: Vec<&str> = request_items_help.lines().collect();
    let mut rs = false;
    for i in 0..lines.len() {
        if lines[i].is_empty() {
            let prev = lines[i - 1].chars().take_while(|&x| x == ' ').count();
            let next = lines[i + 1].chars().take_while(|&x| x == ' ').count();
            if prev != next && next > 0 {
                if !rs {
                    request_items_roff.control("RS", ["8"]);
                    rs = true;
                }
                request_items_roff.control("TP", ["4"]);
            } else if prev != next && next == 0 {
                request_items_roff.control("RE", []);
                request_items_roff.text(vec![roman("")]);
                request_items_roff.control("RS", []);
            } else {
                request_items_roff.text(vec![roman(lines[i])]);
            }
        } else {
            request_items_roff.text(vec![roman(lines[i].trim())]);
        }
    }
    request_items_roff.control("RE", []);

    let mut options_roff = Roff::new();
    let non_pos_items = items
        .iter()
        .filter(|a| !a.is_positional())
        .collect::<Vec<_>>();

    for opt in non_pos_items {
        let mut header = vec![];
        if let Some(short) = opt.get_short() {
            header.push(bold(format!("-{}", short)));
        }
        if let Some(long) = opt.get_long() {
            if !header.is_empty() {
                header.push(roman(", "));
            }
            header.push(bold(format!("--{}", long)));
        }
        if opt.get_action().takes_values() {
            let value_name = &opt.get_value_names().unwrap();
            if opt.get_long().is_some() {
                header.push(roman("="));
            } else {
                header.push(roman(" "));
            }

            if opt.get_id() == "auth" {
                header.push(italic("USER"));
                header.push(roman("["));
                header.push(italic(":PASS"));
                header.push(roman("] | "));
                header.push(italic("TOKEN"));
            } else {
                header.push(italic(value_name.join(" ")));
            }
        }
        let mut body = vec![];

        let mut help = opt
            .get_long_help()
            .or_else(|| opt.get_help())
            .expect("option is missing help")
            .to_string();
        if !help.ends_with('.') {
            help.push('.')
        }
        body.push(roman(help));

        let possible_values = opt.get_possible_values();
        if !possible_values.is_empty()
            && !opt.is_hide_possible_values_set()
            && opt.get_id() != "pretty"
        {
            let possible_values_text = format!(
                "\n\n[possible values: {}]",
                possible_values
                    .iter()
                    .map(|v| v.get_name())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            body.push(roman(possible_values_text));
        }
        options_roff.control("TP", ["4"]);
        options_roff.text(header);
        options_roff.text(body);
    }

    let mut manpage = MAN_TEMPLATE.to_string();

    let current_date = {
        // https://reproducible-builds.org/docs/source-date-epoch/
        let now = match std::env::var("SOURCE_DATE_EPOCH") {
            Ok(val) => DateTime::from_unix_timestamp(val.parse::<i64>().unwrap()).unwrap(),
            Err(_) => DateTime::now_utc(),
        };
        let (year, month, day) = now.date().to_calendar_date();
        format!("{}-{:02}-{:02}", year, u8::from(month), day)
    };

    manpage = manpage.replace("{{date}}", &current_date);
    manpage = manpage.replace("{{version}}", app.get_version().unwrap());
    manpage = manpage.replace("{{request_items}}", request_items_roff.to_roff().trim());
    manpage = manpage.replace("{{options}}", options_roff.to_roff().trim());

    print!("{manpage}");
}
