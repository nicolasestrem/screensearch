//! Selector system for locating UI elements
//!
//! Provides a Playwright-inspired API for building selectors to find UI elements.

use std::collections::BTreeMap;
use std::fmt;

/// Selector for locating UI elements in the accessibility tree
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Selector {
    /// Select by element role (e.g., "button", "edit", "window")
    Role { role: String, name: Option<String> },
    /// Select by automation ID
    Id(String),
    /// Select by name/label
    Name(String),
    /// Select by visible text content
    Text(String),
    /// Select by XPath-like path expression
    Path(String),
    /// Select by multiple attributes
    Attributes(BTreeMap<String, String>),
    /// Chain multiple selectors (hierarchical search)
    Chain(Vec<Selector>),
}

impl Selector {
    /// Create a selector by role
    ///
    /// # Example
    /// ```
    /// use screen_automation::Selector;
    ///
    /// let button = Selector::role("button");
    /// let named_button = Selector::role("button").with_name("Submit");
    /// ```
    pub fn role(role: impl Into<String>) -> SelectorBuilder {
        SelectorBuilder {
            selector: Selector::Role {
                role: role.into(),
                name: None,
            },
        }
    }

    /// Create a selector by automation ID
    pub fn id(id: impl Into<String>) -> Self {
        Selector::Id(id.into())
    }

    /// Create a selector by name
    pub fn name(name: impl Into<String>) -> Self {
        Selector::Name(name.into())
    }

    /// Create a selector by text content
    pub fn text(text: impl Into<String>) -> Self {
        Selector::Text(text.into())
    }

    /// Create a selector by path expression
    pub fn path(path: impl Into<String>) -> Self {
        Selector::Path(path.into())
    }

    /// Create a selector with multiple attributes
    pub fn attributes() -> AttributeSelectorBuilder {
        AttributeSelectorBuilder {
            attributes: BTreeMap::new(),
        }
    }

    /// Chain this selector with another (hierarchical search)
    pub fn then(self, next: Selector) -> Self {
        match self {
            Selector::Chain(mut selectors) => {
                selectors.push(next);
                Selector::Chain(selectors)
            }
            _ => Selector::Chain(vec![self, next]),
        }
    }
}

impl fmt::Display for Selector {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Selector::Role { role, name: None } => write!(f, "role:{}", role),
            Selector::Role {
                role,
                name: Some(name),
            } => write!(f, "role:{}[name='{}']", role, name),
            Selector::Id(id) => write!(f, "#{}", id),
            Selector::Name(name) => write!(f, "name:{}", name),
            Selector::Text(text) => write!(f, "text:{}", text),
            Selector::Path(path) => write!(f, "{}", path),
            Selector::Attributes(attrs) => {
                let pairs: Vec<_> = attrs
                    .iter()
                    .map(|(k, v)| format!("{}='{}'", k, v))
                    .collect();
                write!(f, "[{}]", pairs.join(", "))
            }
            Selector::Chain(selectors) => {
                let parts: Vec<_> = selectors.iter().map(|s| s.to_string()).collect();
                write!(f, "{}", parts.join(" > "))
            }
        }
    }
}

impl From<&str> for Selector {
    fn from(s: &str) -> Self {
        // Parse string into selector
        if s.starts_with('#') {
            Selector::Id(s[1..].to_string())
        } else if s.starts_with("text:") {
            Selector::Text(s[5..].to_string())
        } else if s.starts_with("name:") {
            Selector::Name(s[5..].to_string())
        } else if s.contains(':') {
            // Parse role:name format
            let parts: Vec<&str> = s.splitn(2, ':').collect();
            if parts.len() == 2 {
                Selector::Role {
                    role: parts[0].to_string(),
                    name: Some(parts[1].to_string()),
                }
            } else {
                Selector::Name(s.to_string())
            }
        } else {
            // Default to name selector
            Selector::Name(s.to_string())
        }
    }
}

impl From<String> for Selector {
    fn from(s: String) -> Self {
        s.as_str().into()
    }
}

/// Builder for role-based selectors
#[derive(Debug, Clone)]
pub struct SelectorBuilder {
    selector: Selector,
}

impl SelectorBuilder {
    /// Add a name constraint to the selector
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        if let Selector::Role {
            role: _,
            name: ref mut n,
        } = self.selector
        {
            *n = Some(name.into());
        }
        self
    }

    /// Build the final selector
    pub fn build(self) -> Selector {
        self.selector
    }
}

impl From<SelectorBuilder> for Selector {
    fn from(builder: SelectorBuilder) -> Self {
        builder.build()
    }
}

/// Builder for attribute-based selectors
#[derive(Debug, Clone)]
pub struct AttributeSelectorBuilder {
    attributes: BTreeMap<String, String>,
}

impl AttributeSelectorBuilder {
    /// Add an attribute constraint
    pub fn with(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.attributes.insert(key.into(), value.into());
        self
    }

    /// Build the final selector
    pub fn build(self) -> Selector {
        Selector::Attributes(self.attributes)
    }
}

impl From<AttributeSelectorBuilder> for Selector {
    fn from(builder: AttributeSelectorBuilder) -> Self {
        builder.build()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_selector_parsing() {
        assert_eq!(
            Selector::from("#myButton"),
            Selector::Id("myButton".to_string())
        );

        assert_eq!(
            Selector::from("text:Click Me"),
            Selector::Text("Click Me".to_string())
        );

        assert_eq!(
            Selector::from("button:Submit"),
            Selector::Role {
                role: "button".to_string(),
                name: Some("Submit".to_string()),
            }
        );
    }

    #[test]
    fn test_selector_builder() {
        let selector = Selector::role("button").with_name("Submit").build();

        assert_eq!(
            selector,
            Selector::Role {
                role: "button".to_string(),
                name: Some("Submit".to_string()),
            }
        );
    }

    #[test]
    fn test_selector_chain() {
        let chain = Selector::role("window")
            .build()
            .then(Selector::role("button").build());

        match chain {
            Selector::Chain(selectors) => {
                assert_eq!(selectors.len(), 2);
            }
            _ => panic!("Expected Chain selector"),
        }
    }

    #[test]
    fn test_selector_display() {
        assert_eq!(Selector::role("button").build().to_string(), "role:button");

        assert_eq!(
            Selector::role("button")
                .with_name("Submit")
                .build()
                .to_string(),
            "role:button[name='Submit']"
        );

        assert_eq!(Selector::id("myId").to_string(), "#myId");
    }
}
