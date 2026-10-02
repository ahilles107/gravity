//! Browser tool calls, read as what they did to a page: the bot's own browser
//! (Playwright MCP) and, where allowed, the owner's Chrome (Claude in Chrome).

use serde_json::Value;

/// Whose browser a tool drives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Browser {
    /// The bot's own browser.
    Own,
    /// The owner's Chrome, through the Claude in Chrome extension.
    OwnersChrome,
}

impl Browser {
    pub fn as_str(self) -> &'static str {
        match self {
            Browser::Own => "own",
            Browser::OwnersChrome => "owners_chrome",
        }
    }
}

/// Which browser a tool drives, and its name without the server prefix, or
/// `None` for anything that is not a browser tool.
pub fn browser_tool(name: &str) -> Option<(Browser, &str)> {
    let own = format!("mcp__{}__", crate::browser::setup::SERVER);
    if let Some(tool) = name.strip_prefix(own.as_str()) {
        return Some((Browser::Own, tool.strip_prefix("browser_").unwrap_or(tool)));
    }
    name.strip_prefix("mcp__claude-in-chrome__")
        .map(|tool| (Browser::OwnersChrome, tool.trim_end_matches("_mcp")))
}

/// A title and a detail line for a browser step.
pub fn describe(tool: &str, input: &Value) -> (String, Option<String>) {
    let field = |key: &str| input.get(key).and_then(Value::as_str);
    let url = field("url");
    let element = field("element").or_else(|| field("query"));
    let at = || {
        let point = |key: &str| input.get(key).and_then(Value::as_f64);
        match (point("x"), point("y")) {
            (Some(x), Some(y)) => Some(format!("at {x:.0}, {y:.0}")),
            _ => input
                .get("coordinate")
                .and_then(Value::as_array)
                .map(|c| format!("at {}, {}", c[0], c.get(1).unwrap_or(&Value::Null))),
        }
    };
    match tool {
        "navigate" => (
            format!("Opened {}", url.map(host).unwrap_or("a page")),
            url.map(str::to_string),
        ),
        "navigate_back" => ("Went back".to_string(), None),
        "click" | "mouse_click_xy" | "hover" => {
            let verb = if tool == "hover" {
                "Hovered"
            } else {
                "Clicked"
            };
            match element {
                Some(element) => (format!("{verb} {element}"), None),
                None => (verb.to_string(), at()),
            }
        }
        "type" | "fill_form" | "form_input" => (
            format!("Typed into {}", element.unwrap_or("a field")),
            field("text").map(str::to_string),
        ),
        "press_key" => (format!("Pressed {}", field("key").unwrap_or("a key")), None),
        "tabs" | "tabs_create" | "tabs_close" | "tabs_context" => {
            let action = field("action").unwrap_or(match tool {
                "tabs_create" => "new",
                "tabs_close" => "close",
                _ => "list",
            });
            let title = match action {
                "new" => "Opened a tab",
                "close" => "Closed a tab",
                "select" => "Switched tabs",
                _ => "Listed tabs",
            };
            (title.to_string(), url.map(str::to_string))
        }
        "take_screenshot" => ("Took a screenshot".to_string(), None),
        "snapshot" | "read_page" | "get_page_text" => ("Read the page".to_string(), None),
        "computer" => (
            format!("Used the mouse: {}", field("action").unwrap_or("action")),
            at(),
        ),
        "evaluate" | "javascript_tool" => ("Ran script on the page".to_string(), None),
        other => (other.replace('_', " "), None),
    }
}

fn host(url: &str) -> &str {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    rest.split('/').next().unwrap_or(rest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn tells_the_bots_browser_from_the_owners() {
        assert_eq!(
            browser_tool("mcp__playwright__browser_navigate"),
            Some((Browser::Own, "navigate"))
        );
        assert_eq!(
            browser_tool("mcp__claude-in-chrome__tabs_create_mcp"),
            Some((Browser::OwnersChrome, "tabs_create"))
        );
        assert_eq!(browser_tool("mcp__gravity-bus__send_message"), None);
    }

    #[test]
    fn says_what_happened_to_the_page() {
        let (title, sub) = describe("navigate", &json!({"url": "https://example.com/a"}));
        assert_eq!(title, "Opened example.com");
        assert_eq!(sub.as_deref(), Some("https://example.com/a"));
        assert_eq!(
            describe("click", &json!({"element": "Sign in"})).0,
            "Clicked Sign in"
        );
        assert_eq!(
            describe("mouse_click_xy", &json!({"x": 10.0, "y": 20.0})),
            ("Clicked".to_string(), Some("at 10, 20".to_string()))
        );
        assert_eq!(
            describe("tabs", &json!({"action": "new"})).0,
            "Opened a tab"
        );
    }
}
