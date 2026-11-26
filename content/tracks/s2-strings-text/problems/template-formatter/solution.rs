use std::collections::HashMap;

#[derive(Debug, PartialEq)]
pub enum TemplateError {
    Unknown(String),
    Unclosed(usize),
    StrayBrace(usize),
}

pub fn render(template: &str, vars: &HashMap<&str, &str>) -> Result<String, TemplateError> {
    let mut out = String::with_capacity(template.len());
    let mut chars = template.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        match c {
            '{' => {
                if chars.next_if(|&(_, n)| n == '{').is_some() {
                    out.push('{');
                    continue;
                }
                let close = i + template[i..].find('}').ok_or(TemplateError::Unclosed(i))?;
                let name = &template[i + 1..close];
                let value = vars
                    .get(name)
                    .ok_or_else(|| TemplateError::Unknown(name.to_string()))?;
                out.push_str(value);
                while chars.next_if(|&(j, _)| j <= close).is_some() {}
            }
            '}' => {
                if chars.next_if(|&(_, n)| n == '}').is_none() {
                    return Err(TemplateError::StrayBrace(i));
                }
                out.push('}');
            }
            _ => out.push(c),
        }
    }
    Ok(out)
}
