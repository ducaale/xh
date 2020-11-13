use structopt::clap::arg_enum;
use structopt::StructOpt;

/// Yet another HTTPie clone
#[derive(StructOpt, Debug)]
#[structopt(name = "yahc")]
pub struct Opt {
    /// Print the whole request as well as the response.
    #[structopt(short = "v", long)]
    pub verbose: bool,

    /// Construct HTTP requests without sending them anywhere.
    #[structopt(long)]
    pub offline: bool,

    /// (default) Data items from the command line are serialized as a JSON object.
    #[structopt(short = "j", long)]
    pub json: bool,

    /// Data items from the command line are serialized as form fields.
    #[structopt(short = "f", long)]
    pub form: bool,

    #[structopt(short = "a", long)]
    pub auth: Option<String>,

    /// Controls output processing.
    #[structopt(long, possible_values = &Pretty::variants(), case_insensitive = true)]
    pub pretty: Option<Pretty>,

    /// Output coloring style.
    #[structopt(short = "s", long = "style", possible_values = &Theme::variants(), case_insensitive = true)]
    pub theme: Option<Theme>,

    /// Specify the auth mechanism.
    #[structopt(short = "A", long = "auth-type")]
    pub auth_type: Option<String>,

    /// The default scheme to use if not specified in the URL.
    #[structopt(long = "default-scheme")]
    pub default_scheme: Option<String>,

    /// The HTTP method to be used for the request.
    #[structopt(name = "METHOD", possible_values = &Method::variants(), case_insensitive = true)]
    pub method: Method,

    #[structopt(name = "URL")]
    pub url: String,

    /// Optional key-value pairs to be included in the request.
    #[structopt(name = "REQUEST_ITEM")]
    pub request_items: Vec<String>,
}

// TODO: add remaining methods
arg_enum! {
    #[derive(Debug, Clone)]
    pub enum Method {
        GET,
        POST,
        PUT,
        PATCH,
        DELETE
    }
}

impl From<Method> for reqwest::Method {
    fn from(method: Method) -> Self {
        match method {
            Method::GET => reqwest::Method::GET,
            Method::POST => reqwest::Method::POST,
            Method::PUT => reqwest::Method::PUT,
            Method::PATCH => reqwest::Method::PATCH,
            Method::DELETE => reqwest::Method::DELETE,
        }
    }
}

// TODO: rename this to format?
arg_enum! {
    #[derive(Debug, PartialEq, Clone)]
    pub enum Pretty {
        All, Colors, Format, None
    }
}

arg_enum! {
    #[derive(Debug, PartialEq, Clone)]
    pub enum Theme {
        Auto, Solarized
    }
}
