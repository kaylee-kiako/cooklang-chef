pub mod analysis;
mod context;
pub mod convert;
pub mod error;
pub mod metadata;
pub mod model;
pub mod parser;
pub mod quantity;

use bitflags::bitflags;
use convert::Converter;
use error::{CookResult, CooklangWarning};
use model::Recipe;

bitflags! {
    pub struct Extensions: u32 {
        const MULTINE_STEPS        = 0b00000001;
        const INGREDIENT_MODIFIERS = 0b00000010;
        const INGREDIENT_NOTE      = 0b00000100;
        const INGREDIENT_ALIAS     = 0b00001000;
        const SECTIONS             = 0b00010000;
        const ADVANCED_UNITS       = 0b00100000;
        const MODES                = 0b01000000;
        const TEMPERATURE          = 0b10000000;

        const INGREDIENT_ALL = Self::INGREDIENT_MODIFIERS.bits
                             | Self::INGREDIENT_ALIAS.bits
                             | Self::INGREDIENT_NOTE.bits;
    }
}

impl Default for Extensions {
    fn default() -> Self {
        Self::all()
    }
}

#[derive(Default, Clone, Debug)]
pub struct CooklangParser {
    extensions: Extensions,
    warnings_as_errors: bool,
    converter: Option<Converter>,
}

impl CooklangParser {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_converter(&mut self, converter: Converter) -> &mut Self {
        self.converter = Some(converter);
        self
    }

    pub fn with_extensions(&mut self, extensions: Extensions) -> &mut Self {
        self.extensions = extensions;
        self
    }

    pub fn warnings_as_errors(&mut self, as_err: bool) -> &mut Self {
        self.warnings_as_errors = as_err;
        self
    }

    pub fn parse<'a>(
        &self,
        input: &'a str,
        recipe_name: &str,
    ) -> CookResult<(Recipe<'a>, Vec<CooklangWarning>)> {
        let mut warn = Vec::new();

        let (ast, w) = parser::parse(input, self.extensions, self.warnings_as_errors)?;
        warn.extend(w.into_iter().map(CooklangWarning::from));
        let (content, w) = analysis::analyze_ast(
            input,
            ast,
            self.extensions,
            self.converter.as_ref(),
            self.warnings_as_errors,
        )?;
        warn.extend(w.into_iter().map(CooklangWarning::from));

        Ok((
            Recipe {
                name: recipe_name.to_string(),
                metadata: content.metadata,
                sections: content.sections,
                ingredients: content.ingredients,
                cookware: content.cookware,
                timers: content.timers,
            },
            warn,
        ))
    }
}

pub fn parse<'a>(
    input: &'a str,
    recipe_name: &str,
) -> CookResult<(Recipe<'a>, Vec<CooklangWarning>)> {
    CooklangParser::new().parse(input, recipe_name)
}