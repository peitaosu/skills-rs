#[derive(Debug, Clone)]
pub struct ContextMessage {
    pub label: String,
    pub content: String,
    pub protected: bool,
}

#[derive(Debug, Default)]
pub struct ConversationContext {
    messages: Vec<ContextMessage>,
}

impl ConversationContext {
    pub fn push(&mut self, label: impl Into<String>, content: impl Into<String>, protected: bool) {
        self.messages.push(ContextMessage {
            label: label.into(),
            content: content.into(),
            protected,
        });
    }

    pub fn compact_with_budget(&mut self, keep_unprotected: usize) {
        if keep_unprotected == 0 {
            self.messages.retain(|message| message.protected);
            return;
        }

        let mut protected = Vec::new();
        let mut unprotected = Vec::new();

        for message in self.messages.drain(..) {
            if message.protected {
                protected.push(message);
            } else {
                unprotected.push(message);
            }
        }

        if unprotected.len() > keep_unprotected {
            unprotected.drain(0..unprotected.len() - keep_unprotected);
        }

        protected.extend(unprotected);
        self.messages = protected;
    }

    pub fn compact_with_token_budget(&mut self, max_unprotected_tokens: usize) {
        if max_unprotected_tokens == 0 {
            self.messages.retain(|message| message.protected);
            return;
        }

        let mut protected = Vec::new();
        let mut unprotected = Vec::new();

        for message in self.messages.drain(..) {
            if message.protected {
                protected.push(message);
            } else {
                unprotected.push(message);
            }
        }

        let mut token_budget = max_unprotected_tokens;
        let mut kept = Vec::new();
        for message in unprotected.into_iter().rev() {
            let message_tokens = estimate_tokens(&message.content);
            if kept.is_empty() || token_budget >= message_tokens {
                token_budget = token_budget.saturating_sub(message_tokens);
                kept.push(message);
            }
        }
        kept.reverse();

        protected.extend(kept);
        self.messages = protected;
    }

    pub fn len(&self) -> usize {
        self.messages.len()
    }

    pub fn dump(&self) -> String {
        if self.messages.is_empty() {
            return "(empty context)".to_owned();
        }

        self.messages
            .iter()
            .map(|message| format!("[{}] {}", message.label, message.content))
            .collect::<Vec<_>>()
            .join("\n---\n")
    }
}

pub fn estimate_tokens(text: &str) -> usize {
    text.chars().count().div_ceil(4)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_budget_keeps_recent_unprotected_messages() {
        let mut context = ConversationContext::default();
        context.push("old", "a".repeat(400), false);
        context.push("recent", "b".repeat(40), false);
        context.compact_with_token_budget(20);
        assert_eq!(context.len(), 1);
        assert_eq!(context.messages[0].label, "recent");
    }
}
