//! Curated motivational quotes for the session-boundary screens.
//!
//! A "start your day" set shows on entry (opening the console) and a "wind
//! down" set on exit (leaving the last container). One is picked at random per
//! appearance.

/// A quote and its attribution.
pub struct Quote {
    pub text: &'static str,
    pub author: &'static str,
}

/// Shown on entry — beginnings, momentum, getting to work.
pub const START_QUOTES: &[Quote] = &[
    Quote {
        text: "The secret of getting ahead is getting started.",
        author: "Mark Twain",
    },
    Quote {
        text: "Well begun is half done.",
        author: "Aristotle",
    },
    Quote {
        text: "The way to get started is to quit talking and begin doing.",
        author: "Walt Disney",
    },
    Quote {
        text: "Whether you think you can or you think you can't, you're right.",
        author: "Henry Ford",
    },
    Quote {
        text: "It always seems impossible until it's done.",
        author: "Nelson Mandela",
    },
    Quote {
        text: "Simplicity is the soul of efficiency.",
        author: "Austin Freeman",
    },
    Quote {
        text: "Make it work, make it right, make it fast.",
        author: "Kent Beck",
    },
    Quote {
        text: "The best way to predict the future is to invent it.",
        author: "Alan Kay",
    },
    Quote {
        text: "Action is the foundational key to all success.",
        author: "Pablo Picasso",
    },
    Quote {
        text: "First, solve the problem. Then, write the code.",
        author: "John Johnson",
    },
];

/// Shown on exit — reflection, rest, finishing well.
pub const END_QUOTES: &[Quote] = &[
    Quote {
        text: "Finish each day and be done with it.",
        author: "Ralph Waldo Emerson",
    },
    Quote {
        text: "Rest is not idleness.",
        author: "John Lubbock",
    },
    Quote {
        text: "What is done cannot be undone, but it can be learned from.",
        author: "proverb",
    },
    Quote {
        text: "It does not matter how slowly you go as long as you do not stop.",
        author: "Confucius",
    },
    Quote {
        text: "Done is better than perfect.",
        author: "Sheryl Sandberg",
    },
    Quote {
        text: "Almost everything will work again if you unplug it for a few minutes.",
        author: "Anne Lamott",
    },
    Quote {
        text: "The quieter you become, the more you can hear.",
        author: "Ram Dass",
    },
    Quote {
        text: "You can't pour from an empty cup.",
        author: "proverb",
    },
    Quote {
        text: "Progress, not perfection.",
        author: "proverb",
    },
    Quote {
        text: "Tomorrow is another day to ship.",
        author: "the construct",
    },
];

/// Pick a quote from `set`, seeded by the wall clock so it varies per run
/// without needing an RNG dependency. An empty set yields `None`.
#[must_use]
pub fn pick(set: &'static [Quote]) -> Option<&'static Quote> {
    if set.is_empty() {
        return None;
    }
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos());
    set.get(nanos as usize % set.len())
}
