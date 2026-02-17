pub const RESET: &str = "\x1b[0m";
pub const HEADER: &str = "\x1b[38;5;197;1m";
pub const MANDATORY: &str = "\x1b[38;5;196;1m";
pub const OPTIONAL: &str = "\x1b[38;5;226;1m";
pub const SUCCESS: &str = "\x1b[38;5;46;1m";
pub const INFO: &str = "\x1b[38;5;51;1m";
pub const WARN: &str = "\x1b[38;5;208;1m";
pub const PROMPT: &str = "\x1b[38;5;219;1m";

pub fn paint(color: &str, text: impl AsRef<str>) -> String {
    format!("{}{}{}", color, text.as_ref(), RESET)
}

pub fn banner(title: &str) {
    let top = "==============================================================";
    println!("{}{}{}", HEADER, top, RESET);
    println!("{}{}{}", HEADER, title, RESET);
    println!("{}{}{}", HEADER, top, RESET);
}
