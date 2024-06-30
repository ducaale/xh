//! We used to use syntect for all of our coloring and we still use syntect-compatible
//! files to store themes.
//!
//! But we've started coloring some things manually for better control (and potentially
//! for better efficiency). This macro loads colors from themes and exposes them as
//! fields on a struct. See [`super::headers`] for an example.

macro_rules! palette {
    {
        $vis:vis struct $name:ident {
            $($color:ident: $scopes:expr,)*
        }
    } => {
        $vis struct $name {
            $(pub $color: ::termcolor::ColorSpec,)*
            #[allow(unused)]
            pub default: ::termcolor::ColorSpec,
        }

        impl From<&::syntect::highlighting::Theme> for $name {
            fn from(theme: &::syntect::highlighting::Theme) -> Self {
                let highlighter = ::syntect::highlighting::Highlighter::new(theme);
                let mut parsed_scopes = ::std::vec::Vec::new();
                Self {
                    $($color: $crate::formatting::palette::util::extract_color(
                        &highlighter,
                        &$scopes,
                        &mut parsed_scopes,
                    ),)*
                    default: $crate::formatting::palette::util::extract_default(theme),
                }
            }
        }
    }
}

pub(crate) use palette;

pub(crate) mod util {
    use syntect::{
        highlighting::{Highlighter, Theme},
        parsing::Scope,
    };
    use termcolor::ColorSpec;

    use crate::formatting::{convert_color, convert_style};

    #[inline(never)]
    pub(crate) fn extract_color(
        highlighter: &Highlighter,
        scopes: &[&str],
        parsebuf: &mut Vec<Scope>,
    ) -> ColorSpec {
        parsebuf.clear();
        parsebuf.extend(scopes.iter().map(|s| s.parse::<Scope>().unwrap()));
        let style = highlighter.style_for_stack(parsebuf);
        convert_style(style)
    }

    #[inline(never)]
    pub(crate) fn extract_default(theme: &Theme) -> ColorSpec {
        let mut color = ColorSpec::new();
        if let Some(foreground) = theme.settings.foreground {
            color.set_fg(convert_color(foreground));
        }
        color
    }
}
