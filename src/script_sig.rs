#[derive(Debug)]
pub struct ScriptSig {
    content: Vec<u8>,
}

impl ScriptSig {
    pub fn new(content: Vec<u8>) -> Self {
        ScriptSig {
            content: content.clone(),
        }
    }
}