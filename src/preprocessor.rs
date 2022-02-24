extern crate pulldown_cmark;
extern crate pulldown_cmark_to_cmark;
extern crate semver;

use mdbook::book::Book;
use mdbook::book::BookItem;
use mdbook::book::Chapter;
use mdbook::errors::Error;
use mdbook::preprocess::CmdPreprocessor;
use mdbook::preprocess::Preprocessor;
use mdbook::preprocess::PreprocessorContext;
use pulldown_cmark::Options;

use crate::svgbob::*;
use crate::Result;

/// Svgbob preprocessor for mdbook.
pub struct Bob;

impl Bob {
    pub fn new() -> Self {
        Self
    }

    pub fn handle_preprocessing(&self) -> Result {
        use std::io::stdin;
        use std::io::stdout;

        use semver::Version;
        use semver::VersionReq;

        let (ctx, book) = CmdPreprocessor::parse_input(stdin())?;
        let current = Version::parse(&ctx.mdbook_version)?;
        let built = VersionReq::parse(&format!("~{}", mdbook::MDBOOK_VERSION))?;

        if ctx.mdbook_version != mdbook::MDBOOK_VERSION && !built.matches(&current) {
            warn!(
                "The {} plugin was built against version {} of mdbook, \
				      but we're being called from version {}, so may be incompatible.",
                self.name(),
                mdbook::MDBOOK_VERSION,
                ctx.mdbook_version
            );
        }
        let processed_book = self.run(&ctx, book)?;
        serde_json::to_writer(stdout(), &processed_book)?;
        Ok(())
    }
}

impl Preprocessor for Bob {
    fn name(&self) -> &str {
        "svgbob"
    }
    fn run(&self, ctx: &PreprocessorContext, mut book: Book) -> Result<Book, Error> {
        let settings = ctx.config.get_preprocessor(self.name()).map(cfg_to_settings).unwrap_or_default();

        book.for_each_mut(|item| {
            if let BookItem::Chapter(chapter) = item {
                let _ = process_code_blocks(chapter, &settings)
                    .map(|s| {
                        chapter.content = s;
                        trace!("chapter '{}' processed", &chapter.name);
                    })
                    .map_err(|err| {
                        error!("{}", err);
                    });
            }
        });
        Ok(book)
    }

    fn supports_renderer(&self, renderer: &str) -> bool {
        renderer != "not-supported"
    }
}

/// Find code-blocks \`\`\`bob, produce svg and place it instead code.
fn process_code_blocks(chapter: &mut Chapter, settings: &Settings) -> Result<String, std::fmt::Error> {
    use pulldown_cmark::CodeBlockKind;
    use pulldown_cmark::CowStr;
    use pulldown_cmark::Event;
    use pulldown_cmark::Parser;
    use pulldown_cmark::Tag;
    use pulldown_cmark_to_cmark::cmark;

    enum State {
        None,
        Open,
        Closing,
    }

    let mut state = State::None;
    let mut buf = String::with_capacity(chapter.content.len());

    // Fix(by xp): new() use a default Options does not parse table.
    // But table text `| xx | yy |` will be escaped with leading back slash by `cmark()`
    let events = Parser::new_ext(&chapter.content, Options::all())
        .map(|e| {
            use CodeBlockKind::*;
            use CowStr::*;
            use Event::*;
            use State::*;
            use Tag::CodeBlock;
            use Tag::Paragraph;

            debug!("event: {:?}", e);

            match (&e, &mut state) {
                (Start(CodeBlock(Fenced(Borrowed("bob")))), None) => {
                    state = Open;
                    Some(Start(Paragraph))
                }

                (Text(Borrowed(text)), Open) => {
                    state = Closing;
                    Some(Html(bob_handler(text, settings).into()))
                }

                (End(CodeBlock(Fenced(Borrowed("bob")))), Closing) => {
                    state = None;
                    Some(End(Paragraph))
                }
                _ => Some(e),
            }
        })
        .filter_map(|e| e);
    cmark(events, &mut buf).map(|_| buf)
}

#[cfg(test)]
mod tests {
    #[test]
    fn process_code_blocks() {
        use super::process_code_blocks;
        use super::Chapter;
        use super::Settings;

        let settings = Settings::default();
        let mut chapter = Chapter::new("test", "```bob\n-->\n```".to_owned(), ".", Vec::with_capacity(0));
        let result = process_code_blocks(&mut chapter, &settings).unwrap();
        assert!(result.contains("<svg"));
        assert!(result.contains("<line"));
        assert!(result.contains("#triangle"));
    }
}
