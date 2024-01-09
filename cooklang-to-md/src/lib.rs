//! Format a recipe as markdown

use std::{fmt::Write, io};

use cooklang::{
    convert::Converter,
    metadata::{IndexMap, Metadata},
    model::{Item, Section, Step},
    ScaledRecipe,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error("Error serializing YAML frontmatter")]
    Metadata(
        #[from]
        #[source]
        serde_yaml::Error,
    ),
}

pub type Result<T = ()> = std::result::Result<T, Error>;

/// Options for [`print_md_with_options`]
///
/// This implements [`Serialize`] and [`Deserialize`], so you can embed it in
/// other configuration.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[non_exhaustive]
pub struct Options {
    /// Show the tags in the markdown body
    ///
    /// They will apear just after the title.
    ///
    /// The tags will have the following format:
    /// ```md
    /// #tag1 #tag2 #tag3
    /// ```
    pub tags: bool,
    /// Show the description in the markdown body
    ///
    /// It will appear as a blockquotes just after the tags (if its enabled and
    /// there are any tags; if not, after the title)
    pub description: bool,
    /// Make every step a regular paragraph
    ///
    /// A `cooklang` extensions allows to add paragraphs between steps. Because
    /// some `Markdown` parser may not be able to set the start number of the
    /// list, step numbers may be wrong. With this option enabled, all steps are
    /// paragraphs because the number is escaped like:
    /// ```md
    /// 1\. Step.
    /// ```
    pub escape_step_numbers: bool,
    /// Add the name of the recipe to the front-matter
    ///
    /// A key `name` in the metadata has preference over this.
    pub front_matter_name: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            tags: true,
            description: true,
            escape_step_numbers: false,
            front_matter_name: true,
        }
    }
}

/// Writes a recipe in Markdown format
///
/// This is an alias for [`print_md_with_options`] where the options are the
/// default value.
pub fn print_md(
    recipe: &ScaledRecipe,
    name: &str,
    converter: &Converter,
    writer: impl io::Write,
) -> Result {
    print_md_with_options(recipe, name, Options::default(), converter, writer)
}

/// Writes a recipe in Markdown format
///
/// The metadata of the recipe will be in a YAML front-matter. Some special keys
/// like `autor` or `servings` will be mappings or sequences instead of text if
/// they were parsed correctly.
///
/// The [`Options`] are used to further customize the output. See it's
/// documentation to know about them.
pub fn print_md_with_options(
    recipe: &ScaledRecipe,
    name: &str,
    opts: Options,
    converter: &Converter,
    mut writer: impl io::Write,
) -> Result {
    frontmatter(&mut writer, &recipe.metadata, name, &opts)?;

    writeln!(writer, "# {}", name)?;

    if opts.tags && !recipe.metadata.tags.is_empty() {
        writeln!(writer)?;
        for (i, tag) in recipe.metadata.tags.iter().enumerate() {
            write!(writer, "#{tag}")?;
            if i < recipe.metadata.tags.len() - 1 {
                write!(writer, " ")?;
            }
        }
        writeln!(writer)?;
    }

    if opts.description {
        if let Some(desc) = &recipe.metadata.description {
            writeln!(writer)?;
            print_wrapped_with_options(&mut writer, desc, |o| {
                o.initial_indent("> ").subsequent_indent("> ")
            })?;
            writeln!(writer)?;
        }
    }

    ingredients(&mut writer, recipe, converter)?;
    cookware(&mut writer, recipe)?;
    sections(&mut writer, recipe, &opts)?;

    Ok(())
}

fn frontmatter(
    mut w: impl io::Write,
    metadata: &Metadata,
    name: &str,
    opts: &Options,
) -> Result<()> {
    if metadata.map.is_empty() {
        return Ok(());
    }

    let mut map = IndexMap::new();

    if opts.front_matter_name {
        // add name, will be overrided if other given
        map.insert("name", name.into());
    }

    // add all the raw metadata entries
    for (key, val) in &metadata.map {
        map.insert(key.as_str(), val.to_string().into());
    }

    // overwrite special values if any and correct
    macro_rules! override_special_key {
        ($meta:ident, $thing:ident) => {
            if let Some(val) = &$meta.$thing {
                map.insert(
                    stringify!($thing),
                    serde_yaml::to_value(val.clone()).unwrap(),
                );
            }
        };
        ($meta:ident, $thing:ident : not_empty) => {
            if !$meta.$thing.is_empty() {
                map.insert(
                    stringify!($thing),
                    serde_yaml::to_value($meta.$thing.clone()).unwrap(),
                );
            }
        };
    }
    override_special_key!(metadata, author);
    override_special_key!(metadata, source);
    override_special_key!(metadata, time);
    override_special_key!(metadata, servings);
    override_special_key!(metadata, tags : not_empty);

    const FRONTMATTER_FENCE: &str = "---";
    writeln!(w, "{}", FRONTMATTER_FENCE)?;
    serde_yaml::to_writer(&mut w, &map)?;
    writeln!(w, "{}\n", FRONTMATTER_FENCE)?;
    Ok(())
}

fn ingredients(w: &mut impl io::Write, recipe: &ScaledRecipe, converter: &Converter) -> Result {
    if recipe.ingredients.is_empty() {
        return Ok(());
    }

    writeln!(w, "## Ingredients")?;

    for entry in recipe.group_ingredients(converter) {
        let ingredient = entry.ingredient;

        if !ingredient.modifiers().should_be_listed() {
            continue;
        }

        write!(w, "- ")?;
        if !entry.quantity.is_empty() {
            write!(w, "*{}* ", entry.quantity)?;
        }

        write!(w, "{}", ingredient.display_name())?;

        if ingredient.modifiers().is_optional() {
            write!(w, " (optional)")?;
        }

        if let Some(note) = &ingredient.note {
            write!(w, " ({note})")?;
        }
        writeln!(w)?;
    }
    writeln!(w)?;

    Ok(())
}

fn cookware(w: &mut impl io::Write, recipe: &ScaledRecipe) -> Result {
    if recipe.cookware.is_empty() {
        return Ok(());
    }

    writeln!(w, "## Cookware")?;
    for item in recipe.group_cookware() {
        let cw = item.cookware;
        write!(w, "- ")?;
        if !item.amount.is_empty() {
            write!(w, "*{}* ", item.amount)?;
        }
        write!(w, "{}", cw.display_name())?;

        if cw.modifiers().is_optional() {
            write!(w, " (optional)")?;
        }

        if let Some(note) = &cw.note {
            write!(w, " ({note})")?;
        }
        writeln!(w)?;
    }

    writeln!(w)?;
    Ok(())
}

fn sections(w: &mut impl io::Write, recipe: &ScaledRecipe, opts: &Options) -> Result<()> {
    writeln!(w, "## Steps")?;
    for (idx, section) in recipe.sections.iter().enumerate() {
        w_section(w, section, recipe, idx + 1, opts)?;
    }
    Ok(())
}

fn w_section(
    w: &mut impl io::Write,
    section: &Section,
    recipe: &ScaledRecipe,
    idx: usize,
    opts: &Options,
) -> Result {
    if section.name.is_some() || recipe.sections.len() > 1 {
        if let Some(name) = &section.name {
            writeln!(w, "### {name}")?;
        } else {
            writeln!(w, "### Section {idx}")?;
        }
    }
    for content in &section.content {
        match content {
            cooklang::Content::Step(step) => w_step(w, step, recipe, opts)?,
            cooklang::Content::Text(text) => print_wrapped(w, text)?,
        };
        writeln!(w)?;
    }
    Ok(())
}

fn w_step(w: &mut impl io::Write, step: &Step, recipe: &ScaledRecipe, opts: &Options) -> Result {
    let mut step_str = step.number.to_string();
    if opts.escape_step_numbers {
        step_str.push_str("\\. ")
    } else {
        step_str.push_str(". ")
    }

    for item in &step.items {
        match item {
            Item::Text { value } => step_str.push_str(value),
            &Item::Ingredient { index } => {
                let igr = &recipe.ingredients[index];
                step_str.push_str(igr.display_name().as_ref());
            }
            &Item::Cookware { index } => {
                let cw = &recipe.cookware[index];
                step_str.push_str(&cw.name);
            }
            &Item::Timer { index } => {
                let t = &recipe.timers[index];
                if let Some(name) = &t.name {
                    write!(&mut step_str, "({name})").unwrap();
                }
                if let Some(quantity) = &t.quantity {
                    write!(&mut step_str, "{}", quantity).unwrap();
                }
            }
            &Item::InlineQuantity { index } => {
                let q = &recipe.inline_quantities[index];
                write!(&mut step_str, "{}", q.value).unwrap();
                if let Some(u) = q.unit_text() {
                    step_str.push_str(u);
                }
            }
        }
    }
    print_wrapped(w, &step_str)?;
    Ok(())
}

fn print_wrapped(w: &mut impl io::Write, text: &str) -> Result {
    print_wrapped_with_options(w, text, |o| o)
}

static TERM_WIDTH: once_cell::sync::Lazy<usize> =
    once_cell::sync::Lazy::new(|| textwrap::termwidth().min(80));

fn print_wrapped_with_options<F>(w: &mut impl io::Write, text: &str, f: F) -> Result
where
    F: FnOnce(textwrap::Options) -> textwrap::Options,
{
    let options = f(textwrap::Options::new(*TERM_WIDTH));
    let lines = textwrap::wrap(text, options);
    for line in lines {
        writeln!(w, "{}", line)?;
    }
    Ok(())
}
