use std::io;

use clap_complete::Shell;
use clap_complete_nushell::Nushell;

use crate::cli::Cli;
use crate::cli::Generate;

const MAN_TEMPLATE: &str = include_str!("../doc/man-template.roff");
const MD_TEMPLATE: &str = include_str!("../doc/md-template.md");

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
            use std::io::Write;
            let mut buf = Vec::new();
            clap_complete::generate(Shell::Fish, &mut app, bin_name, &mut buf);
            let mut stdout = io::stdout();
            // Based on https://github.com/fish-shell/fish-shell/blob/1e61e6492db879ba6c32013f901d84b067ca22eb/share/completions/curl.fish#L1-L6
            let preamble = format!(
                r#"# Complete paths after @ in options:
function __{bin_name}_complete_data
    string match -qr '^(?<prefix>.*@)(?<path>.*)' -- (commandline -ct)
    printf '%s\n' -- $prefix(__fish_complete_path $path)
end
complete -c {bin_name} -n 'string match -qr "@" -- (commandline -ct)' -kxa "(__{bin_name}_complete_data)"

"#,
            );
            stdout.write_all(preamble.as_bytes()).unwrap();
            stdout.write_all(&buf).unwrap();
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
        Generate::ManMarkdown => {
            generate_markdown(&mut app);
        }
    }
}

fn generate_markdown(app: &mut clap::Command) {
    let items: Vec<_> = app.get_arguments().filter(|i| !i.is_hide_set()).collect();

    let mut request_items = String::new();
    let request_items_help = items
        .iter()
        .find(|opt| opt.get_id() == "raw_rest_args")
        .expect("request_items not found")
        .get_long_help()
        .expect("request_items is missing help")
        .to_string()
        .replace("\"", "`");

    let mut indent = false;
    for line in parse_help(&request_items_help) {
        match line {
            ParsedHelp::Definition(term, Some(description)) => {
                request_items.push_str(&format!("  - `{term}`: {description}\n"))
            }
            ParsedHelp::Definition(term, None) => {
                request_items.push_str(&format!("  - `{term}`\n"))
            }
            ParsedHelp::Line(line) if indent => request_items.push_str(&format!("    {line}\n")),
            ParsedHelp::Line(line) => request_items.push_str(&format!("  {line}\n")),
            ParsedHelp::Indent => indent = true,
            ParsedHelp::DeIndent => indent = false,
        }
    }

    let mut options = String::new();
    let non_pos_items = items
        .iter()
        .filter(|a| !a.is_positional())
        .collect::<Vec<_>>();

    for opt in non_pos_items {
        let mut header = String::new();
        if let Some(short) = opt.get_short() {
            header.push_str(&format!("`-{short}`"));
        }
        if let Some(long) = opt.get_long() {
            if !header.is_empty() {
                header.push_str(", ");
            }
            header.push_str(&format!("`--{long}`"));
        }
        if opt.get_action().takes_values() {
            header.pop();
            let value_name = &opt.get_value_names().unwrap();
            if opt.get_long().is_some() {
                header.push('=');
            } else {
                header.push(' ');
            }
            header.push_str(&value_name.join(" "));
            header.push('`')
        }

        let mut body = String::new();

        let mut help = opt
            .get_long_help()
            .or_else(|| opt.get_help())
            .expect("option is missing help")
            .to_string()
            .replace("\"", "`");
        if !help.ends_with('.') {
            help.push('.')
        }

        let mut indent = false;
        for line in parse_help(&help) {
            match line {
                ParsedHelp::Definition(term, Some(description)) => {
                    body.push_str(&format!("  - `{term}`: {description}\n"))
                }
                ParsedHelp::Definition(term, None) => body.push_str(&format!("  - `{term}`\n")),
                ParsedHelp::Line(line) if indent => body.push_str(&format!("    {line}\n")),
                ParsedHelp::Line(line) => body.push_str(&format!("  {line}\n")),
                ParsedHelp::Indent => indent = true,
                ParsedHelp::DeIndent => indent = false,
            }
        }

        let possible_values = opt.get_possible_values();
        if !possible_values.is_empty()
            && !opt.is_hide_possible_values_set()
            && opt.get_id() != "pretty"
        {
            let possible_values_text = format!(
                "\n  [possible values: {}]\n",
                possible_values
                    .iter()
                    .map(|v| format!("`{}`", v.get_name()))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            body.push_str(&possible_values_text);
        }
        options.push_str(&format!("- {header}: {}\n", body.trim_start()));
    }

    let mut manpage = MD_TEMPLATE.to_string();

    manpage = manpage.replace("{{request_items}}", request_items.trim_end());
    manpage = manpage.replace("{{options}}", options.trim());

    print!("{manpage}");
}

fn generate_manpages(app: &mut clap::Command) {
    use roff::{Roff, bold, italic, roman};
    use time::OffsetDateTime as DateTime;

    let items: Vec<_> = app.get_arguments().filter(|i| !i.is_hide_set()).collect();

    let mut request_items_roff = Roff::new();
    let request_items_help = items
        .iter()
        .find(|opt| opt.get_id() == "raw_rest_args")
        .expect("request_items not found")
        .get_long_help()
        .expect("request_items is missing help")
        .to_string();

    for line in parse_help(&request_items_help) {
        match line {
            ParsedHelp::Definition(term, Some(description)) => {
                request_items_roff.control("TP", ["4"]);
                request_items_roff.text([roman(term)]);
                request_items_roff.text([roman(description)]);
            }
            ParsedHelp::Definition(_, None) => {
                unreachable!()
            }
            ParsedHelp::Line(line) => {
                request_items_roff.text([roman(line)]);
            }
            ParsedHelp::Indent => {
                request_items_roff.control("RS", ["8"]);
            }
            ParsedHelp::DeIndent => {
                request_items_roff.control("RE", []);
                request_items_roff.control("IP", []);
            }
        }
    }

    let mut options_roff = Roff::new();
    let non_pos_items = items
        .iter()
        .filter(|a| !a.is_positional())
        .collect::<Vec<_>>();

    for opt in non_pos_items {
        options_roff.control("TP", ["4"]);

        let mut header = vec![];

        if let Some(short) = opt.get_short() {
            header.push(bold(format!("-{short}")));
        }
        if let Some(long) = opt.get_long() {
            if !header.is_empty() {
                header.push(roman(", "));
            }
            header.push(bold(format!("--{long}")));
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
        options_roff.text(header);

        let mut body = vec![];

        let mut help = opt
            .get_long_help()
            .or_else(|| opt.get_help())
            .expect("option is missing help")
            .to_string();
        if !help.ends_with('.') {
            help.push('.')
        }

        for line in parse_help(&help) {
            match line {
                ParsedHelp::Definition(term, Some(description)) => {
                    options_roff.control("TP", ["8"]);
                    options_roff.text([roman(term)]);
                    options_roff.text([roman(description)]);
                }
                ParsedHelp::Definition(term, None) => {
                    options_roff.control("IP", ["\"\"", "0"]);
                    options_roff.text([roman(term)]);
                }
                ParsedHelp::Line(line) => {
                    options_roff.text([roman(line)]);
                }
                ParsedHelp::Indent => {
                    options_roff.control("RS", ["8"]);
                }
                ParsedHelp::DeIndent => {
                    options_roff.control("RE", []);
                    options_roff.control("IP", []);
                }
            }
        }

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

#[derive(Debug, PartialEq)]
enum ParsedHelp<'a> {
    Line(&'a str),
    Definition(&'a str, Option<&'a str>),
    Indent,
    DeIndent,
}

fn parse_help(body: &str) -> Vec<ParsedHelp<'_>> {
    let mut parsed: Vec<ParsedHelp> = Vec::new();

    let mut indent = false;

    for line in body.lines() {
        if let Some(line) = line.strip_prefix("    ") {
            if !indent {
                parsed.push(ParsedHelp::Indent);
                indent = true;
            }
            if let Some(line) = line.strip_prefix("    ") {
                if let Some(ParsedHelp::Definition(_, description)) = parsed.last_mut() {
                    description.replace(line);
                } else {
                    parsed.push(ParsedHelp::Line(line))
                }
            } else if let Some((term, description)) = line.split_once("   ") {
                parsed.push(ParsedHelp::Definition(term, Some(description.trim_start())))
            } else {
                parsed.push(ParsedHelp::Definition(line, None))
            }
        } else {
            if indent && !line.is_empty() {
                parsed.push(ParsedHelp::DeIndent);
                indent = false;
            }
            parsed.push(ParsedHelp::Line(line.trim_start()))
        }
    }

    if indent {
        parsed.push(ParsedHelp::DeIndent);
    }

    parsed
}

#[cfg(test)]
mod tests {
    use super::*;
    use indoc::indoc;

    #[test]
    fn parse_help_with_definition() {
        let parsed = parse_help(indoc! {r#"
            Controls output processing. Possible values are:

                all      (default) Enable both coloring and formatting
                colors   Apply syntax highlighting to output
                format   Pretty-print json and sort headers
                none     Disable both coloring and formatting

            Defaults to "format" if the NO_COLOR env is set and to "none" if stdout is not tty.
        "#});
        assert_eq!(
            parsed,
            [
                ParsedHelp::Line("Controls output processing. Possible values are:"),
                ParsedHelp::Line(""),
                ParsedHelp::Indent,
                ParsedHelp::Definition(
                    "all",
                    Some("(default) Enable both coloring and formatting")
                ),
                ParsedHelp::Definition("colors", Some("Apply syntax highlighting to output")),
                ParsedHelp::Definition("format", Some("Pretty-print json and sort headers")),
                ParsedHelp::Definition("none", Some("Disable both coloring and formatting")),
                ParsedHelp::Line(""),
                ParsedHelp::DeIndent,
                ParsedHelp::Line(
                    "Defaults to \"format\" if the NO_COLOR env is set and to \"none\" if stdout is not tty."
                )
            ]
        );
    }

    #[test]
    fn parse_help_with_list() {
        let parsed = parse_help(indoc! {"
            String specifying what the output should contain

                'H' request headers
                'B' request body

            Example: --print=Hb
        "});

        assert_eq!(
            parsed,
            [
                ParsedHelp::Line("String specifying what the output should contain"),
                ParsedHelp::Line(""),
                ParsedHelp::Indent,
                ParsedHelp::Definition("'H' request headers", None),
                ParsedHelp::Definition("'B' request body", None),
                ParsedHelp::Line(""),
                ParsedHelp::DeIndent,
                ParsedHelp::Line("Example: --print=Hb"),
            ]
        );
    }

    #[test]
    fn parse_help_with_extended_definition() {
        let parsed = parse_help(indoc! {r#"
            The separator is used to determine the type:

                key==value
                    Add a query string to the URL.

                key:=value
                    Add a field with a literal JSON value to the request body.

                    Example: enabled:=true

            A backslash can be used to escape special characters.
        "#});

        assert_eq!(
            parsed,
            [
                ParsedHelp::Line("The separator is used to determine the type:"),
                ParsedHelp::Line(""),
                ParsedHelp::Indent,
                ParsedHelp::Definition("key==value", Some("Add a query string to the URL.")),
                ParsedHelp::Line(""),
                ParsedHelp::Definition(
                    "key:=value",
                    Some("Add a field with a literal JSON value to the request body.")
                ),
                ParsedHelp::Line(""),
                ParsedHelp::Line("Example: enabled:=true"),
                ParsedHelp::Line(""),
                ParsedHelp::DeIndent,
                ParsedHelp::Line("A backslash can be used to escape special characters."),
            ]
        );
    }
}
