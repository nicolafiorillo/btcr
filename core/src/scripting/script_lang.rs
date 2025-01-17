use std::fmt::{Display, Formatter};

use crate::std_lib::{
    std_result::StdResult,
    vector::{bytes_to_hex_string, hex_string_to_bytes},
};

use super::{context::Context, opcode::*, token::Token};

#[derive(Debug, Clone)]
pub struct ScriptLang(Vec<Token>);

impl ScriptLang {
    // TODO: refactor
    pub fn deserialize(data: &[u8], length: u64, offset: usize) -> StdResult<Self> {
        let mut tokens: Vec<Token> = vec![];

        let mut i = offset as u64;
        let max = length + offset as u64;

        while i < max {
            let first = data[i as usize];
            if OP_ELEMENTS_RANGE.contains(&(first as OpCode)) {
                i += 1;

                let start = i as usize;
                let end = start + first as usize;

                let bytes = &data[start..end];
                tokens.push(Token::Element(bytes.to_vec()));

                i += first as u64;
            } else if first == OP_PUSHDATA1 as u8 {
                // TODO: NOT TESTED
                i += 1;
                let len = data[i as usize];

                i += 1;
                let start = i as usize;
                let end = start + len as usize;

                let bytes = &data[start..end];
                tokens.push(Token::Element(bytes.to_vec()));

                i += len as u64;
            } else if first == OP_PUSHDATA2 as u8 {
                // TODO: NOT TESTED
                let len_bytes = &data[(i + 1) as usize..(i + 3) as usize];
                let len = u16::from_le_bytes([len_bytes[0], len_bytes[1]]);

                i += 2;

                let start = (i + 1) as usize;
                let end = start + len as usize;

                let bytes = &data[start..end];
                tokens.push(Token::Element(bytes.to_vec()));

                i += 1 + len as u64;
            } else if first == OP_PUSHDATA4 as u8 {
                // TODO: NOT TESTED
                let len_bytes = &data[(i + 1) as usize..(i + 5) as usize];
                let len = u32::from_le_bytes([len_bytes[0], len_bytes[1], len_bytes[2], len_bytes[3]]);

                i += 4;

                let start = (i + 1) as usize;
                let end = start + len as usize;

                let bytes = &data[start..end];
                tokens.push(Token::Element(bytes.to_vec()));

                i += 1 + len as u64;
            } else {
                tokens.push(Token::Command(first as OpCode));
                i += 1;
            }
        }
        Ok(ScriptLang(tokens))
    }

    pub fn from_tokens(tokens: Vec<Token>) -> Self {
        ScriptLang(tokens)
    }

    pub fn tokens(&self) -> Vec<Token> {
        let Self(tokens) = self;
        tokens.to_vec()
    }

    pub fn prepend(self, tokens: Vec<Token>) -> Self {
        let Self(old_tokens) = self;
        let mut new_tokens = tokens;
        new_tokens.extend(old_tokens);

        ScriptLang(new_tokens)
    }

    pub fn from_representation(repr: &str) -> StdResult<Self> {
        let trimmed_repr = repr.trim();
        let mut items: Vec<Token> = vec![];

        let tokens = trimmed_repr.split(' ').collect::<Vec<&str>>();

        for item in tokens {
            if let Some(op_code) = OP_TO_FN.iter().position(|op| op.name == item) {
                items.push(Token::Command(op_code));
            } else {
                match hex_string_to_bytes(item) {
                    Ok(bytes) => items.push(Token::Element(bytes)),
                    Err(_) => Err("invalid_script_representation")?,
                };
            }
        }

        Ok(ScriptLang(items))
    }

    pub fn representation(&self) -> String {
        let Self(items) = self;

        let mut repr = String::new();
        for item in items {
            match item {
                Token::Element(bytes) => {
                    let e = bytes_to_hex_string(bytes);
                    repr.push_str(&e);
                }
                Token::Command(op_code) => {
                    repr.push_str((*OP_TO_FN)[*op_code].name);
                }
            }
            repr.push(' ');
        }

        repr.trim_end().to_string()
    }

    pub fn serialize(&self) -> StdResult<Vec<u8>> {
        let Self(tokens) = self;

        let raw = ScriptLang::raw_serialize(tokens)?;
        Ok(raw)
    }

    pub fn evaluate<'a>(&'a self, context: &'a mut Context) -> StdResult<bool> {
        while !context.tokens_are_over() {
            let executing = context.executing();

            let token = context.next_token();
            log::debug!("Token (exec: {}): {:}", executing, token);

            if !executing && !token.is_op_branch_condition() {
                continue;
            }

            match token {
                Token::Element(bytes) => {
                    let e = Token::Element(bytes.to_vec());
                    context.stack_push(e);
                }
                Token::Command(op_code) => {
                    if *op_code > OPS_LENGTH {
                        Err("invalid_opcode")?;
                    }

                    ((*OP_TO_FN)[*op_code].exec)(context)?;
                }
            }
        }

        Ok(context.is_valid())
    }

    pub fn combine(left: Self, right: Self) -> Self {
        let Self(left_items) = left;
        let Self(right_items) = right;

        ScriptLang([left_items, right_items].concat())
    }

    fn raw_serialize(tokens: &[Token]) -> StdResult<Vec<u8>> {
        let mut raw: Vec<u8> = vec![];

        for token in tokens {
            match token {
                Token::Element(bytes) => {
                    let len = bytes.len();
                    if len <= 75 {
                        raw.push(len as u8);
                    } else if len <= 0xFF {
                        raw.push(OP_PUSHDATA1 as u8);
                        raw.push(len as u8);
                    } else if len <= 0x208 {
                        raw.push(OP_PUSHDATA2 as u8);
                        raw.extend(len.to_le_bytes().iter());
                    } else if len < 0x100000000 {
                        Err("push_data_4_is_deprecated")?;
                    } else {
                        Err("element_too_long")?;
                    }

                    raw.extend(bytes);
                }
                Token::Command(op_code) => {
                    raw.push(*op_code as u8);
                }
            }
        }

        Ok(raw)
    }
}

impl Display for ScriptLang {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:}", self.representation())
    }
}

#[cfg(test)]
mod script_test {
    use crate::{
        flags::{network::Network, sighash::SigHash},
        hashing::hash160::hash160,
        scripting::{opcode::*, standard, token::*},
        std_lib::varint::{decode, encode},
        std_lib::{integer_extended::IntegerExtended, vector::hex_string_to_bytes},
        wallet::key::new,
    };

    use rug::{integer::Order, Integer};

    use super::*;

    #[test]
    fn represent() {
        let script = ScriptLang::from_tokens(vec![
            Token::Element(vec![0x00]),
            Token::Element(vec![0x01]),
            Token::Command(OP_CHECKSIG),
        ]);

        assert_eq!(format!("{}", script), "00 01 OP_CHECKSIG");
    }

    #[test]
    fn from_representation() {
        let expected = vec![
            Token::Element(vec![0x00]),
            Token::Element(vec![0x01]),
            Token::Command(OP_CHECKSIG),
        ];

        let script = ScriptLang::from_representation("00 01 OP_CHECKSIG").unwrap();
        let ScriptLang(tokens) = script;

        assert_eq!(expected, tokens);
    }

    #[test]
    fn serialize() {
        let pubkey = hex_string_to_bytes("04887387e452b8eacc4acfde10d9aaf7f6d9a0f975aabb10d006e4da568744d06c61de6d95231cd89026e286df3b6ae4a894a3378e393e93a0f45b666329a0ae34").unwrap();
        let signature = hex_string_to_bytes("3045022000eff69ef2b1bd93a66ed5219add4fb51e11a840f404876325a1e8ffe0529a2c022100c7207fee197d27c618aea621406f6bf5ef6fca38681d82b2f06fddbdce6feab601").unwrap();

        let pubkey_script = ScriptLang::from_tokens(vec![Token::Element(pubkey), Token::Command(OP_CHECKSIG)]);

        let signature_script = ScriptLang::from_tokens(vec![Token::Element(signature)]);
        let script = ScriptLang::combine(signature_script, pubkey_script);

        let mut serialized = script.serialize().unwrap();

        let length = encode(serialized.len() as u64);
        serialized = [length, serialized].concat();

        let expected = hex_string_to_bytes("8c483045022000eff69ef2b1bd93a66ed5219add4fb51e11a840f404876325a1e8ffe0529a2c022100c7207fee197d27c618aea621406f6bf5ef6fca38681d82b2f06fddbdce6feab6014104887387e452b8eacc4acfde10d9aaf7f6d9a0f975aabb10d006e4da568744d06c61de6d95231cd89026e286df3b6ae4a894a3378e393e93a0f45b666329a0ae34ac").unwrap();

        assert_eq!(serialized, expected);
    }

    #[test]
    fn deserialize() {
        let data = hex_string_to_bytes("8c483045022000eff69ef2b1bd93a66ed5219add4fb51e11a840f404876325a1e8ffe0529a2c022100c7207fee197d27c618aea621406f6bf5ef6fca38681d82b2f06fddbdce6feab6014104887387e452b8eacc4acfde10d9aaf7f6d9a0f975aabb10d006e4da568744d06c61de6d95231cd89026e286df3b6ae4a894a3378e393e93a0f45b666329a0ae34ac").unwrap();

        let var_int = decode(&data, 0).unwrap();
        let script = ScriptLang::deserialize(&data, var_int.value, var_int.length).unwrap();

        let ScriptLang(tokens) = script;

        assert_eq!(tokens.len(), 3);
        assert_eq!(tokens[0], Token::Element(hex_string_to_bytes("3045022000eff69ef2b1bd93a66ed5219add4fb51e11a840f404876325a1e8ffe0529a2c022100c7207fee197d27c618aea621406f6bf5ef6fca38681d82b2f06fddbdce6feab601").unwrap()));
        assert_eq!(tokens[1], Token::Element(hex_string_to_bytes("04887387e452b8eacc4acfde10d9aaf7f6d9a0f975aabb10d006e4da568744d06c61de6d95231cd89026e286df3b6ae4a894a3378e393e93a0f45b666329a0ae34").unwrap()));
        assert_eq!(tokens[2], Token::Command(OP_CHECKSIG));
    }

    #[test]
    fn evaluate_odd_number() {
        let script = ScriptLang::from_representation("F").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context);

        let op = context.stack_pop_as_element().unwrap();

        assert_eq!(op, Token::Element(hex_string_to_bytes("0F").unwrap()));
    }

    #[test]
    fn evaluate_checksig() {
        let z: Integer = Integer::from_hex_str("7C076FF316692A3D7EB3C3BB0F8B1488CF72E1AFCD929E29307032997A838A3D");
        let pubkey = hex_string_to_bytes("04887387e452b8eacc4acfde10d9aaf7f6d9a0f975aabb10d006e4da568744d06c61de6d95231cd89026e286df3b6ae4a894a3378e393e93a0f45b666329a0ae34").unwrap();
        let signature = hex_string_to_bytes("3045022000eff69ef2b1bd93a66ed5219add4fb51e11a840f404876325a1e8ffe0529a2c022100c7207fee197d27c618aea621406f6bf5ef6fca38681d82b2f06fddbdce6feab601").unwrap();

        let pubkey_script = ScriptLang::from_tokens(vec![Token::Element(pubkey), Token::Command(OP_CHECKSIG)]);

        let signature_script = ScriptLang::from_tokens(vec![Token::Element(signature)]);
        let script = ScriptLang::combine(signature_script, pubkey_script);

        let tokens = script.tokens();
        let mut context = Context::new(tokens, z);

        assert!(script.evaluate(&mut context).unwrap());
    }

    #[test]
    fn evaluate_checkmultisig_1_to_1() {
        let z: Integer = Integer::from_hex_str("6CD7818C2ED773A1B19348FEACA92AD664B45CD0");
        let pubkey = hex_string_to_bytes("02a130c1e1ffa137cf50824ece45fb648ce88cb5570870dc10cfdc8c5f30946861").unwrap();
        let signature = hex_string_to_bytes("3045022100bebe0c00a59a6c01231790fe8034508c06904289de0e3ddccb897d9cf5794b0202205e1ff2d6f060524bd7da2a598f5205759ef0911a695407999965527ba9629a2501").unwrap();

        let script_sig = ScriptLang::from_tokens(vec![Token::Command(OP_0), Token::Element(signature)]);
        let script_multisig = ScriptLang::from_tokens(vec![
            Token::Command(OP_1),
            Token::Element(pubkey),
            Token::Command(OP_1),
            Token::Command(OP_CHECKMULTISIG),
        ]);

        let script = ScriptLang::combine(script_sig, script_multisig);

        let tokens = script.tokens();
        let mut context = Context::new(tokens, z);

        assert!(script.evaluate(&mut context).unwrap());
    }

    //
    // OP_X
    //
    macro_rules! evaluate_op_n {
        ($n:literal, $f:ident, $r:literal) => {
            #[test]
            fn $f() {
                let script = ScriptLang::from_representation($n).unwrap();
                let mut context = Context::new(script.tokens(), Integer::from(0));
                let _valid = script.evaluate(&mut context).unwrap();

                let op = context.stack_pop_as_element().unwrap();

                assert_eq!(op, Token::Element(element_encode($r)));
            }
        };
    }

    evaluate_op_n!("OP_1", evaluate_op_1, 1);
    evaluate_op_n!("OP_2", evaluate_op_2, 2);
    evaluate_op_n!("OP_3", evaluate_op_3, 3);
    evaluate_op_n!("OP_4", evaluate_op_4, 4);
    evaluate_op_n!("OP_5", evaluate_op_5, 5);
    evaluate_op_n!("OP_6", evaluate_op_6, 6);
    evaluate_op_n!("OP_7", evaluate_op_7, 7);
    evaluate_op_n!("OP_8", evaluate_op_8, 8);
    evaluate_op_n!("OP_9", evaluate_op_9, 9);
    evaluate_op_n!("OP_10", evaluate_op_10, 10);
    evaluate_op_n!("OP_11", evaluate_op_11, 11);
    evaluate_op_n!("OP_12", evaluate_op_12, 12);
    evaluate_op_n!("OP_13", evaluate_op_13, 13);
    evaluate_op_n!("OP_14", evaluate_op_14, 14);
    evaluate_op_n!("OP_15", evaluate_op_15, 15);
    evaluate_op_n!("OP_16", evaluate_op_16, 16);

    #[test]
    fn evaluate_0() {
        let script = ScriptLang::from_tokens(vec![Token::Command(OP_0)]);
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        let op = context.stack_pop_as_element().unwrap();

        assert_eq!(op, Token::Element(ELEMENT_ZERO.to_vec()));
    }

    #[test]
    fn evaluate_1negate() {
        let script = ScriptLang::from_tokens(vec![Token::Command(OP_1NEGATE)]);
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        let op = context.stack_pop_as_element().unwrap();

        assert_eq!(op, Token::Element(ELEMENT_ONE_NEGATE.to_vec()));
    }

    #[test]
    fn evaluate_nop() {
        let script = ScriptLang::from_tokens(vec![Token::Command(OP_NOP)]);
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.stack_has_items(0));
    }

    #[test]
    fn evaluate_add_too_long_1() {
        let script = ScriptLang::from_representation("FF FFFFFFFFFF OP_ADD").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let valid = script.evaluate(&mut context);

        assert_eq!("input_length_too_long", valid.expect_err("Err").to_string());
    }

    #[test]
    fn evaluate_add_too_long_2() {
        let script = ScriptLang::from_representation("FFFFFFFFFF FF OP_ADD").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let valid = script.evaluate(&mut context);

        assert_eq!("input_length_too_long", valid.expect_err("Err").to_string());
    }

    #[test]
    fn evaluate_add() {
        let script = ScriptLang::from_tokens(vec![
            Token::Element(vec![0x01]),
            Token::Element(vec![0x02]),
            Token::Command(OP_ADD),
        ]);
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        let op = context.stack_pop_as_element().unwrap();

        assert_eq!(op, Token::Element(vec![0x03]));
    }

    #[test]
    fn evaluate_sub_too_long_1() {
        let script = ScriptLang::from_representation("FF FFFFFFFFFF OP_SUB").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let valid = script.evaluate(&mut context);

        assert_eq!("input_length_too_long", valid.expect_err("Err").to_string());
    }

    #[test]
    fn evaluate_sub_too_long_2() {
        let script = ScriptLang::from_representation("FFFFFFFFFF FF OP_SUB").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let valid = script.evaluate(&mut context);

        assert_eq!("input_length_too_long", valid.expect_err("Err").to_string());
    }

    #[test]
    fn evaluate_sub() {
        let script = ScriptLang::from_tokens(vec![
            Token::Element(vec![0x02]),
            Token::Element(vec![0x01]),
            Token::Command(OP_SUB),
        ]);
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        let op = context.stack_pop_as_element().unwrap();

        assert_eq!(op, Token::Element(vec![0x01]));
    }

    #[test]
    fn evaluate_sub_neg() {
        let script = ScriptLang::from_tokens(vec![
            Token::Element(vec![0x01]),
            Token::Element(vec![0x02]),
            Token::Command(OP_SUB),
        ]);
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        let op = context.stack_pop_as_element().unwrap();

        assert_eq!(op, Token::Element(vec![0x81]));
    }

    #[test]
    fn evaluate_sub_zero() {
        let script = ScriptLang::from_representation("7FFFFFFF 7FFFFFFF OP_SUB").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context);

        let op = context.stack_pop_as_element().unwrap();

        assert_eq!(op, Token::Element(vec![]));
    }

    #[test]
    fn evaluate_sub_ff_81() {
        let script = ScriptLang::from_representation("FF 81 OP_SUB").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context);

        let op = context.stack_pop_as_element().unwrap();

        assert_eq!(op, Token::Element(vec![0xFE]));
    }

    // #[test]
    // fn evaluate_sub_with_overflow() {
    //     let script = ScriptLang::from_representation("7FFFFFFFFFFFFFFF 7FFFFFFFFFFFFFFF OP_SUB").unwrap();
    //     let mut context = Context::new(script.tokens(), Integer::from(0));
    //     let valid = script.evaluate(&mut context);

    //     assert_eq!("overflow", valid.expect_err("Err").to_string());
    // }

    #[test]
    fn evaluate_mul() {
        let script = ScriptLang::from_tokens(vec![
            Token::Element(vec![0x02]),
            Token::Element(vec![0x02]),
            Token::Command(OP_MUL),
        ]);
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        let op = context.stack_pop_as_element().unwrap();

        assert_eq!(op, Token::Element(vec![0x04]));
    }

    #[test]
    fn evaluate_equal_true() {
        let script = ScriptLang::from_tokens(vec![
            Token::Element(vec![0x01]),
            Token::Element(vec![0x01]),
            Token::Command(OP_EQUAL),
        ]);
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        let op = context.stack_pop_as_element().unwrap();

        assert_eq!(op, Token::Element(ELEMENT_TRUE.to_vec()));
    }

    #[test]
    fn evaluate_equal_false() {
        let script = ScriptLang::from_tokens(vec![
            Token::Element(vec![0x01]),
            Token::Element(vec![0x02]),
            Token::Command(OP_EQUAL),
        ]);
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        let op = context.stack_pop_as_element().unwrap();

        assert_eq!(op, Token::Element(ELEMENT_FALSE.to_vec()));
    }

    #[test]
    fn evaluate_if_true() {
        let script = ScriptLang::from_tokens(vec![Token::Element(vec![0x01]), Token::Command(OP_IF)]);
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.stack_has_items(0));
        assert!(context.executing())
    }

    #[test]
    fn evaluate_if_false() {
        let script = ScriptLang::from_tokens(vec![Token::Element(vec![0x00]), Token::Command(OP_IF)]);
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.stack_has_items(0));
        assert!(!context.executing())
    }

    #[test]
    fn evaluate_notif() {
        let script = ScriptLang::from_tokens(vec![Token::Element(vec![0x01]), Token::Command(OP_NOTIF)]);
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.stack_has_items(0));
        assert!(!context.executing())
    }

    #[test]
    fn evaluate_notif_false() {
        let script = ScriptLang::from_tokens(vec![Token::Element(vec![0x00]), Token::Command(OP_NOTIF)]);
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.stack_has_items(0));
        assert!(context.executing())
    }

    #[test]
    fn evaluate_return() {
        let script = ScriptLang::from_tokens(vec![Token::Element(vec![0x01]), Token::Command(OP_RETURN)]);
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let valid = script.evaluate(&mut context);

        assert_eq!("exit_by_return", valid.expect_err("Err").to_string());
    }

    #[test]
    fn evaluate_return_with_data() {
        let script = ScriptLang::from_representation("OP_RETURN FFFF").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let valid = script.evaluate(&mut context);

        let res = context.data().clone();

        assert_eq!(vec![0xFF, 0xFF], res.unwrap());
        assert_eq!("exit_by_return", valid.expect_err("Err").to_string());
    }

    #[test]
    fn evaluate_return_with_too_long_data() {
        let script = ScriptLang::from_representation("OP_RETURN FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF01").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let valid = script.evaluate(&mut context);

        let res = context.data().clone();

        assert_eq!(Vec::<u8>::new(), res.unwrap());
        assert_eq!("return_data_too_long", valid.expect_err("Err").to_string());
    }

    #[test]
    fn evaluate_if_endif() {
        let script = ScriptLang::from_tokens(vec![
            Token::Element(vec![0x01]),
            Token::Command(OP_IF),
            Token::Command(OP_ENDIF),
        ]);
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.stack_has_items(0));
        assert!(context.executing())
    }

    #[test]
    fn evaluate_if_else_endif() {
        let script = ScriptLang::from_tokens(vec![
            Token::Element(vec![0x01]),
            Token::Command(OP_IF),
            Token::Command(OP_ELSE),
            Token::Command(OP_ENDIF),
        ]);
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.stack_has_items(0));
        assert!(context.executing())
    }

    #[test]
    fn evaluate_conditional_script_1() {
        let script = ScriptLang::from_representation("01 00 OP_IF 02 OP_ENDIF").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.stack_has_items(1));

        assert!(context.executing());
        assert!(context.is_valid());
    }

    #[test]
    fn evaluate_conditional_script_2() {
        let script = ScriptLang::from_representation("01 01 OP_IF 02 OP_ENDIF").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.stack_has_items(2));

        assert!(context.executing());
        assert!(context.is_valid());
    }

    #[test]
    fn evaluate_conditional_script_3() {
        let script = ScriptLang::from_representation("00 OP_IF 01 OP_ELSE 00 OP_ENDIF").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.stack_has_items(1));

        assert!(context.executing());
        assert!(!context.is_valid());
    }

    #[test]
    fn evaluate_conditional_script_4() {
        let script = ScriptLang::from_representation("01 OP_IF 01 OP_ELSE 00 OP_ENDIF").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.stack_has_items(1));

        assert!(context.executing());
        assert!(context.is_valid());
    }

    #[test]
    fn evaluate_script_nested_if_1() {
        let script = ScriptLang::from_representation("00 OP_IF 01 OP_IF OP_RETURN OP_ELSE OP_RETURN OP_ELSE OP_RETURN OP_ENDIF OP_ELSE 01 OP_IF 01 OP_ELSE OP_RETURN OP_ELSE 01 OP_ENDIF OP_ELSE OP_RETURN OP_ENDIF OP_ADD 02 OP_EQUAL").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.is_valid());
    }

    #[test]
    fn evaluate_script_nested_if_2() {
        let script = ScriptLang::from_representation("20 OP_IF 00 OP_IF OP_RETURN OP_ELSE 10 OP_ENDIF OP_ELSE 01 OP_IF 01 OP_ELSE OP_RETURN OP_ELSE 01 OP_ENDIF OP_ELSE 30 OP_ENDIF OP_ADD 40 OP_EQUAL").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.is_valid());
    }

    #[test]
    fn evaluate_dup() {
        let script = ScriptLang::from_representation("09 OP_DUP").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.stack_has_items(2));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x09]));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x09]));
    }

    #[test]
    fn evaluate_2dup() {
        let script = ScriptLang::from_representation("0A 0B OP_2DUP").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.stack_has_items(4));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x0B]));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x0A]));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x0B]));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x0A]));
    }

    #[test]
    fn evaluate_3dup() {
        let script = ScriptLang::from_representation("0A 0B 0C OP_3DUP").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.stack_has_items(6));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x0C]));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x0B]));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x0A]));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x0C]));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x0B]));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x0A]));
    }

    #[test]
    fn evaluate_2over() {
        let script = ScriptLang::from_representation("0A 0B 0C 0D OP_2OVER").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.stack_has_items(6));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x0B]));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x0A]));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x0D]));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x0C]));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x0B]));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x0A]));
    }

    #[test]
    fn evaluate_rot() {
        let script = ScriptLang::from_representation("0A 0B 0C OP_ROT").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.stack_has_items(3));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x0A]));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x0C]));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x0B]));
    }

    #[test]
    fn evaluate_2rot() {
        let script = ScriptLang::from_representation("0A 0B 0C 0D 0E 0F OP_2ROT").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.stack_has_items(6));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x0B]));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x0A]));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x0F]));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x0E]));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x0D]));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x0C]));
    }

    #[test]
    fn evaluate_hash160() {
        let script = ScriptLang::from_representation("09 OP_HASH160").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.stack_has_items(1));

        let op = context.stack_pop_as_element().unwrap();

        let expected = hex_string_to_bytes("D6A8A804D5BE366AE5D3A318CDCED1DC1CFE28EA").unwrap();
        assert_eq!(op, Token::Element(expected));
    }

    #[test]
    fn evaluate_hash256() {
        let script = ScriptLang::from_representation("09 OP_HASH256").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.stack_has_items(1));

        let op = context.stack_pop_as_element().unwrap();

        let expected = hex_string_to_bytes("2AD16B189B68E7672A886C82A0550BC531782A3A4CFB2F08324E316BB0F3174D").unwrap();
        assert_eq!(op, Token::Element(expected));
    }

    #[test]
    fn evaluate_sha256() {
        let script = ScriptLang::from_representation("09 OP_SHA256").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.stack_has_items(1));

        let op = context.stack_pop_as_element().unwrap();

        let expected = hex_string_to_bytes("2B4C342F5433EBE591A1DA77E013D1B72475562D48578DCA8B84BAC6651C3CB9").unwrap();
        assert_eq!(op, Token::Element(expected));
    }

    #[test]
    fn evaluate_sha1() {
        let script = ScriptLang::from_representation("09 OP_SHA1").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.stack_has_items(1));

        let op = context.stack_pop_as_element().unwrap();

        let expected = hex_string_to_bytes("AC9231DA4082430AFE8F4D40127814C613648D8E").unwrap();
        assert_eq!(op, Token::Element(expected));
    }

    #[test]
    fn evaluate_ripemd160() {
        let script = ScriptLang::from_representation("09 OP_RIPEMD160").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.stack_has_items(1));

        let op = context.stack_pop_as_element().unwrap();

        let expected = hex_string_to_bytes("F9D4B6DA252769BB4DE563C1FF4EECADDBE937F6").unwrap();
        assert_eq!(op, Token::Element(expected));
    }

    #[test]
    fn evaluate_verify_true() {
        let script = ScriptLang::from_representation("09 OP_VERIFY 01").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.is_valid());
        assert!(context.stack_has_items(1));
    }

    #[test]
    fn evaluate_verify_false() {
        let script = ScriptLang::from_representation("00 OP_VERIFY").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let valid = script.evaluate(&mut context);

        assert_eq!("exit_by_failed_verify", valid.expect_err("Err").to_string());
    }

    #[test]
    fn evaluate_equalverify_true() {
        let script = ScriptLang::from_representation("09 09 OP_EQUALVERIFY 01").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.is_valid());
        assert!(context.stack_has_items(1));
    }

    #[test]
    fn evaluate_equalverify_false() {
        let script = ScriptLang::from_representation("09 08 OP_EQUALVERIFY 01").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let valid = script.evaluate(&mut context);

        assert_eq!("exit_by_failed_verify", valid.expect_err("Err").to_string());
    }

    #[test]
    fn evaluate_not_too_long() {
        let script = ScriptLang::from_representation("FFFFFFFFFF OP_NOT").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let valid = script.evaluate(&mut context);

        assert_eq!("input_length_too_long", valid.expect_err("Err").to_string());
    }

    #[test]
    fn evaluate_not_1() {
        let script = ScriptLang::from_representation("00 OP_NOT").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.is_valid());
        assert!(context.stack_has_items(1));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(ELEMENT_ONE.to_vec()));
    }

    #[test]
    fn evaluate_not_2() {
        let script = ScriptLang::from_representation("01 OP_NOT").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(!context.is_valid());
        assert!(context.stack_has_items(1));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(ELEMENT_ZERO.to_vec()));
    }

    #[test]
    fn evaluate_not_3() {
        let script = ScriptLang::from_representation("AA OP_NOT").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(!context.is_valid());
        assert!(context.stack_has_items(1));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(ELEMENT_ZERO.to_vec()));
    }

    #[test]
    fn evaluate_toaltstack() {
        let script = ScriptLang::from_representation("AA OP_TOALTSTACK").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.stack_has_items(0));
        assert!(context.alt_stack_has_items(1));

        let op = context.alt_stack_pop();
        assert_eq!(op, Token::Element(vec![0xAA]));
    }

    #[test]
    fn evaluate_fromaltstack() {
        let script = ScriptLang::from_representation("OP_FROMALTSTACK").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        context.alt_stack_push(Token::Element(vec![0xAA]));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.stack_has_items(1));
        assert!(context.alt_stack_has_items(0));

        let op = context.stack_pop();
        assert_eq!(op, Token::Element(vec![0xAA]));
    }

    #[test]
    fn evaluate_swap() {
        let script = ScriptLang::from_representation("01 02 OP_SWAP").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.is_valid());
        assert!(context.stack_has_items(2));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x01]));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x02]));
    }

    #[test]
    fn evaluate_2swap() {
        let script = ScriptLang::from_representation("0A 0B 0C 0D OP_2SWAP").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.is_valid());
        assert!(context.stack_has_items(4));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x0B]));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x0A]));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x0D]));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x0C]));
    }

    #[test]
    fn evaluate_nip() {
        let script = ScriptLang::from_representation("0A 0B OP_NIP").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.stack_has_items(1));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x0B]));
    }

    #[test]
    fn evaluate_drop() {
        let script = ScriptLang::from_representation("0A 0B 0C OP_DROP").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.stack_has_items(2));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x0B]));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x0A]));
    }

    #[test]
    fn evaluate_2drop() {
        let script = ScriptLang::from_representation("0A 0B 0C OP_2DROP").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.stack_has_items(1));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x0A]));
    }

    #[test]
    fn evaluate_ifdup_non_zero() {
        let script = ScriptLang::from_representation("01 OP_IFDUP").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.stack_has_items(2));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x01]));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x01]));
    }

    #[test]
    fn evaluate_ifdup_non_zero_1() {
        let script = ScriptLang::from_representation("OP_1 OP_IFDUP").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.stack_has_items(2));

        let op = context.stack_pop();
        assert_eq!(op, Token::Element(vec![0x01]));

        let op = context.stack_pop();
        assert_eq!(op, Token::Element(vec![0x01]));
    }

    #[test]
    fn evaluate_ifdup_zero() {
        let script = ScriptLang::from_representation("00 OP_IFDUP").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.stack_has_items(1));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x00]));
    }

    #[test]
    fn evaluate_depth() {
        let script = ScriptLang::from_representation("0A 0B 0C OP_DEPTH").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.stack_has_items(4));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x03]));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x0C]));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x0B]));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x0A]));
    }

    #[test]
    fn evaluate_over() {
        let script = ScriptLang::from_representation("0A 0B 0C OP_OVER").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.stack_has_items(4));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x0B]));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x0C]));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x0B]));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x0A]));
    }

    #[test]
    fn evaluate_pick_1() {
        let script = ScriptLang::from_representation("0A 00 OP_PICK").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.stack_has_items(2));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x0A]));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x0A]));
    }

    #[test]
    fn evaluate_pick_2() {
        let script = ScriptLang::from_representation("0A 0B 01 OP_PICK").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.stack_has_items(3));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x0A]));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x0B]));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x0A]));
    }

    #[test]
    fn evaluate_pick_3() {
        let script = ScriptLang::from_representation("0A 0B 0C 01 OP_PICK").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.stack_has_items(4));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x0B]));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x0C]));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x0B]));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x0A]));
    }

    #[test]
    fn evaluate_pick_4() {
        let script = ScriptLang::from_representation("0A 0B 0C 02 OP_PICK").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.stack_has_items(4));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x0A]));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x0C]));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x0B]));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x0A]));
    }

    #[test]
    fn evaluate_pick_5() {
        let script = ScriptLang::from_representation("0A 0B 0C 0D 03 OP_PICK").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.stack_has_items(5));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x0A]));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x0D]));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x0C]));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x0B]));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x0A]));
    }

    #[test]
    fn evaluate_pick_6() {
        let script = ScriptLang::from_representation("0A 0B 0C 0D 0E 0F 03 OP_PICK").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.stack_has_items(7));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x0C]));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x0F]));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x0E]));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x0D]));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x0C]));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x0B]));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x0A]));
    }

    #[test]
    fn evaluate_roll_1() {
        let script = ScriptLang::from_representation("0A 00 OP_ROLL").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.stack_has_items(1));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x0A]));
    }

    #[test]
    fn evaluate_roll_2() {
        let script = ScriptLang::from_representation("0A 0B 01 OP_ROLL").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.stack_has_items(2));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x0A]));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x0B]));
    }

    #[test]
    fn evaluate_roll_3() {
        let script = ScriptLang::from_representation("0A 0B 0C 01 OP_ROLL").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.stack_has_items(3));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x0B]));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x0C]));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x0A]));
    }

    #[test]
    fn evaluate_roll_4() {
        let script = ScriptLang::from_representation("0A 0B 0C 02 OP_ROLL").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.stack_has_items(3));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x0A]));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x0C]));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x0B]));
    }

    #[test]
    fn evaluate_roll_5() {
        let script = ScriptLang::from_representation("0A 0B 0C 0D 03 OP_ROLL").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.stack_has_items(4));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x0A]));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x0D]));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x0C]));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x0B]));
    }

    #[test]
    fn evaluate_roll_6() {
        let script = ScriptLang::from_representation("0A 0B 0C 0D 0E 0F 03 OP_ROLL").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.stack_has_items(6));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x0C]));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x0F]));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x0E]));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x0D]));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x0B]));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x0A]));
    }

    #[test]
    fn evaluate_tuck() {
        let script = ScriptLang::from_representation("0A 0B OP_TUCK").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.stack_has_items(3));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x0B]));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x0A]));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x0B]));
    }

    #[test]
    fn evaluate_size() {
        let script = ScriptLang::from_representation("FFFFFF OP_SIZE").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.stack_has_items(2));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x03]));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0xFF, 0xFF, 0xFF]));
    }

    #[test]
    fn evaluate_1add_too_long() {
        let script = ScriptLang::from_representation("FFFFFFFFFF OP_1ADD").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let valid = script.evaluate(&mut context);

        assert_eq!("input_length_too_long", valid.expect_err("Err").to_string());
    }

    #[test]
    fn evaluate_1add_0() {
        let script = ScriptLang::from_representation("00 OP_1ADD").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.stack_has_items(1));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x01]));
    }

    #[test]
    fn evaluate_1add_1() {
        let script = ScriptLang::from_representation("01 OP_1ADD").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.stack_has_items(1));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x02]));
    }

    #[test]
    fn evaluate_1add_ff() {
        let script = ScriptLang::from_representation("FF OP_1ADD").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.stack_has_items(1));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0xFE]));
    }

    #[test]
    fn evaluate_1add_7f() {
        let script = ScriptLang::from_representation("7F OP_1ADD").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.stack_has_items(1));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x80, 0x00]));
    }

    #[test]
    fn evaluate_1add_8000() {
        let script = ScriptLang::from_representation("8000 OP_1ADD").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.stack_has_items(1));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x81, 0x00]));
    }

    #[test]
    fn evaluate_1add_81() {
        let script = ScriptLang::from_representation("81 OP_1ADD").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.stack_has_items(1));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![]));
    }

    #[test]
    fn evaluate_1sub_too_long() {
        let script = ScriptLang::from_representation("FFFFFFFFFF OP_1SUB").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let valid = script.evaluate(&mut context);

        assert_eq!("input_length_too_long", valid.expect_err("Err").to_string());
    }

    #[test]
    fn evaluate_1sub_0() {
        let script = ScriptLang::from_representation("00 OP_1SUB").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.stack_has_items(1));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x81]));
    }

    #[test]
    fn evaluate_1sub_1() {
        let script = ScriptLang::from_representation("01 OP_1SUB").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.stack_has_items(1));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![]));
    }

    #[test]
    fn evaluate_1sub_ff() {
        let script = ScriptLang::from_representation("FF OP_1SUB").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.stack_has_items(1));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x80, 0x80]));
    }

    #[test]
    fn evaluate_1sub_81() {
        let script = ScriptLang::from_representation("81 OP_1ADD").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.stack_has_items(1));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![]));
    }

    #[test]
    fn evaluate_1sub_8000() {
        let script = ScriptLang::from_representation("8000 OP_1SUB").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.stack_has_items(1));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x7F]));
    }

    #[test]
    fn evaluate_1sub_82() {
        let script = ScriptLang::from_representation("82 OP_1SUB").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.stack_has_items(1));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x83]));
    }

    #[test]
    fn evaluate_negate_too_long() {
        let script = ScriptLang::from_representation("FFFFFFFFFF OP_NEGATE").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let valid = script.evaluate(&mut context);

        assert_eq!("input_length_too_long", valid.expect_err("Err").to_string());
    }

    #[test]
    fn evaluate_negate_0() {
        let script = ScriptLang::from_representation("00 OP_NEGATE").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.stack_has_items(1));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![]));
    }

    #[test]
    fn evaluate_negate_1() {
        let script = ScriptLang::from_representation("01 OP_NEGATE").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.stack_has_items(1));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x81]));
    }

    #[test]
    fn evaluate_negate_neg1() {
        let script = ScriptLang::from_representation("81 OP_NEGATE").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.stack_has_items(1));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x01]));
    }

    #[test]
    fn evaluate_negate_80() {
        let script = ScriptLang::from_representation("80 OP_NEGATE").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.stack_has_items(1));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![]));
    }

    #[test]
    fn evaluate_negate_ff() {
        let script = ScriptLang::from_representation("FF OP_NEGATE").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.stack_has_items(1));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x7F]));
    }

    #[test]
    fn evaluate_negate_ffff() {
        let script = ScriptLang::from_representation("FFFF OP_NEGATE").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.stack_has_items(1));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0xFF, 0x7F]));
    }

    #[test]
    fn evaluate_abs_too_long() {
        let script = ScriptLang::from_representation("FFFFFFFFFF OP_ABS").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let valid = script.evaluate(&mut context);

        assert_eq!("input_length_too_long", valid.expect_err("Err").to_string());
    }

    #[test]
    fn evaluate_abs_0() {
        let script = ScriptLang::from_representation("00 OP_ABS").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.stack_has_items(1));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![]));
    }

    #[test]
    fn evaluate_abs_1() {
        let script = ScriptLang::from_representation("01 OP_ABS").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.stack_has_items(1));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x01]));
    }

    #[test]
    fn evaluate_abs_neg1() {
        let script = ScriptLang::from_representation("81 OP_ABS").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.stack_has_items(1));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x01]));
    }

    #[test]
    fn evaluate_abs_7f() {
        let script = ScriptLang::from_representation("7F OP_ABS").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.stack_has_items(1));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x7F]));
    }

    #[test]
    fn evaluate_abs_80() {
        let script = ScriptLang::from_representation("80 OP_ABS").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.stack_has_items(1));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![]));
    }

    #[test]
    fn evaluate_abs_ff() {
        let script = ScriptLang::from_representation("FF OP_ABS").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.stack_has_items(1));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x7F]));
    }

    #[test]
    fn evaluate_abs_ffff() {
        let script = ScriptLang::from_representation("FFFF OP_ABS").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.stack_has_items(1));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0xFF, 0x7F]));
    }

    #[test]
    fn evaluate_0notequal_too_long() {
        let script = ScriptLang::from_representation("FFFFFFFFFF OP_0NOTEQUAL").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let valid = script.evaluate(&mut context);

        assert_eq!("input_length_too_long", valid.expect_err("Err").to_string());
    }

    #[test]
    fn evaluate_0notequal_00() {
        let script = ScriptLang::from_representation("00 OP_0NOTEQUAL").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.stack_has_items(1));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(ELEMENT_ZERO.to_vec()));
    }

    #[test]
    fn evaluate_0notequal_01() {
        let script = ScriptLang::from_representation("01 OP_0NOTEQUAL").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.stack_has_items(1));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(ELEMENT_ONE.to_vec()));
    }

    #[test]
    fn evaluate_0notequal_80() {
        let script = ScriptLang::from_representation("80 OP_0NOTEQUAL").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.stack_has_items(1));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(ELEMENT_ZERO.to_vec()));
    }

    #[test]
    fn evaluate_0notequal_ff() {
        let script = ScriptLang::from_representation("ff OP_0NOTEQUAL").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.stack_has_items(1));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(ELEMENT_ONE.to_vec()));
    }

    #[test]
    fn evaluate_booland_1() {
        let script = ScriptLang::from_representation("00 00 OP_BOOLAND").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.stack_has_items(1));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(ELEMENT_ZERO.to_vec()));
    }

    #[test]
    fn evaluate_booland_2() {
        let script = ScriptLang::from_representation("01 01 OP_BOOLAND").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.stack_has_items(1));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(ELEMENT_ONE.to_vec()));
    }

    #[test]
    fn evaluate_booland_3() {
        let script = ScriptLang::from_representation("FF 00 OP_BOOLAND").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.stack_has_items(1));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(ELEMENT_ZERO.to_vec()));
    }

    #[test]
    fn evaluate_booland_4() {
        let script = ScriptLang::from_representation("00 FF OP_BOOLAND").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.stack_has_items(1));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(ELEMENT_ZERO.to_vec()));
    }

    #[test]
    fn evaluate_booland_5() {
        let script = ScriptLang::from_representation("FF FF OP_BOOLAND").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.stack_has_items(1));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(ELEMENT_ONE.to_vec()));
    }

    #[test]
    fn evaluate_boolor_1() {
        let script = ScriptLang::from_representation("00 00 OP_BOOLOR").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.stack_has_items(1));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(ELEMENT_ZERO.to_vec()));
    }

    #[test]
    fn evaluate_boolor_2() {
        let script = ScriptLang::from_representation("01 01 OP_BOOLOR").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.stack_has_items(1));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(ELEMENT_ONE.to_vec()));
    }

    #[test]
    fn evaluate_boolor_3() {
        let script = ScriptLang::from_representation("FF 00 OP_BOOLOR").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.stack_has_items(1));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(ELEMENT_ONE.to_vec()));
    }

    #[test]
    fn evaluate_boolor_4() {
        let script = ScriptLang::from_representation("00 FF OP_BOOLOR").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.stack_has_items(1));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(ELEMENT_ONE.to_vec()));
    }

    #[test]
    fn evaluate_boolor_5() {
        let script = ScriptLang::from_representation("FF FF OP_BOOLOR").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.stack_has_items(1));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(ELEMENT_ONE.to_vec()));
    }

    #[test]
    fn evaluate_numequal_true() {
        let script = ScriptLang::from_representation("01 01 OP_NUMEQUAL").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.stack_has_items(1));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(ELEMENT_ONE.to_vec()));
    }

    #[test]
    fn evaluate_numequal_false_1() {
        let script = ScriptLang::from_representation("01 00 OP_NUMEQUAL").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.stack_has_items(1));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(ELEMENT_ZERO.to_vec()));
    }

    #[test]
    fn evaluate_numequal_false_2() {
        let script = ScriptLang::from_representation("00 01 OP_NUMEQUAL").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.stack_has_items(1));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(ELEMENT_ZERO.to_vec()));
    }

    #[test]
    fn evaluate_numnotequal_true_1() {
        let script = ScriptLang::from_representation("01 00 OP_NUMNOTEQUAL").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.stack_has_items(1));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(ELEMENT_ONE.to_vec()));
    }

    #[test]
    fn evaluate_numnotequal_true_2() {
        let script = ScriptLang::from_representation("00 01 OP_NUMNOTEQUAL").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.stack_has_items(1));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(ELEMENT_ONE.to_vec()));
    }

    #[test]
    fn evaluate_numnotequal_false_1() {
        let script = ScriptLang::from_representation("01 01 OP_NUMNOTEQUAL").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.stack_has_items(1));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(ELEMENT_ZERO.to_vec()));
    }

    #[test]
    fn evaluate_numnotequal_false_2() {
        let script = ScriptLang::from_representation("00 00 OP_NUMNOTEQUAL").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.stack_has_items(1));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(ELEMENT_ZERO.to_vec()));
    }

    #[test]
    fn evaluate_numequalverify_true() {
        let script = ScriptLang::from_representation("09 09 OP_NUMEQUALVERIFY 01").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.is_valid());
        assert!(context.stack_has_items(1));
    }

    #[test]
    fn evaluate_numequalverify_false() {
        let script = ScriptLang::from_representation("09 08 OP_NUMEQUALVERIFY 01").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let valid = script.evaluate(&mut context);

        assert_eq!("exit_by_failed_verify", valid.expect_err("Err").to_string());
    }

    #[test]
    fn evaluate_lessthan() {
        let script = ScriptLang::from_representation("08 09 OP_LESSTHAN").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context);

        assert!(context.is_valid());
        assert!(context.stack_has_items(1));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(ELEMENT_ONE.to_vec()));
    }

    #[test]
    fn evaluate_lessthan_fail() {
        let script = ScriptLang::from_representation("09 08 OP_LESSTHAN").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context);

        assert!(context.stack_has_items(1));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(ELEMENT_ZERO.to_vec()));
    }

    #[test]
    fn evaluate_lessthan_equal() {
        let script = ScriptLang::from_representation("08 08 OP_LESSTHAN").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context);

        assert!(context.stack_has_items(1));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(ELEMENT_ZERO.to_vec()));
    }

    #[test]
    fn evaluate_greaterthan() {
        let script = ScriptLang::from_representation("09 08 OP_GREATERTHAN").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context);

        assert!(context.is_valid());
        assert!(context.stack_has_items(1));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(ELEMENT_ONE.to_vec()));
    }

    #[test]
    fn evaluate_greaterthan_fail() {
        let script = ScriptLang::from_representation("08 09 OP_GREATERTHAN").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context);

        assert!(context.stack_has_items(1));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(ELEMENT_ZERO.to_vec()));
    }

    #[test]
    fn evaluate_greaterthan_equal() {
        let script = ScriptLang::from_representation("08 08 OP_GREATERTHAN").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context);

        assert!(context.stack_has_items(1));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(ELEMENT_ZERO.to_vec()));
    }

    #[test]
    fn evaluate_lessthanorequal() {
        let script = ScriptLang::from_representation("08 09 OP_LESSTHANOREQUAL").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context);

        assert!(context.is_valid());
        assert!(context.stack_has_items(1));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(ELEMENT_ONE.to_vec()));
    }

    #[test]
    fn evaluate_lessthanorequal_fail() {
        let script = ScriptLang::from_representation("09 08 OP_LESSTHANOREQUAL").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context);

        assert!(context.stack_has_items(1));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(ELEMENT_ZERO.to_vec()));
    }

    #[test]
    fn evaluate_lessthanorequal_equal() {
        let script = ScriptLang::from_representation("08 08 OP_LESSTHANOREQUAL").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context);

        assert!(context.stack_has_items(1));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(ELEMENT_ONE.to_vec()));
    }

    #[test]
    fn evaluate_greaterthanorequal() {
        let script = ScriptLang::from_representation("09 08 OP_GREATERTHANOREQUAL").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context);

        assert!(context.is_valid());
        assert!(context.stack_has_items(1));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(ELEMENT_ONE.to_vec()));
    }

    #[test]
    fn evaluate_greaterthanorequal_fail() {
        let script = ScriptLang::from_representation("08 09 OP_GREATERTHANOREQUAL").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context);

        assert!(context.stack_has_items(1));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(ELEMENT_ZERO.to_vec()));
    }

    #[test]
    fn evaluate_greaterthanorequal_equal() {
        let script = ScriptLang::from_representation("08 08 OP_GREATERTHANOREQUAL").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context);

        assert!(context.stack_has_items(1));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(ELEMENT_ONE.to_vec()));
    }

    #[test]
    fn evaluate_generic_script_1() {
        let script = ScriptLang::from_representation("02 OP_DUP OP_DUP OP_MUL OP_ADD OP_6 OP_EQUAL").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.is_valid());
        assert!(context.stack_has_items(1));
    }

    #[test]
    fn evaluate_min_right() {
        let script = ScriptLang::from_representation("09 08 OP_MIN").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context);

        assert!(context.is_valid());
        assert!(context.stack_has_items(1));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x08]));
    }

    #[test]
    fn evaluate_min_left() {
        let script = ScriptLang::from_representation("08 09 OP_MIN").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context);

        assert!(context.is_valid());
        assert!(context.stack_has_items(1));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x08]));
    }

    #[test]
    fn evaluate_min_equal() {
        let script = ScriptLang::from_representation("08 08 OP_MIN").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context);

        assert!(context.is_valid());
        assert!(context.stack_has_items(1));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x08]));
    }

    #[test]
    fn evaluate_max_right() {
        let script = ScriptLang::from_representation("09 08 OP_MAX").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context);

        assert!(context.is_valid());
        assert!(context.stack_has_items(1));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x09]));
    }

    #[test]
    fn evaluate_max_left() {
        let script = ScriptLang::from_representation("08 09 OP_MAX").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context);

        assert!(context.is_valid());
        assert!(context.stack_has_items(1));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x09]));
    }

    #[test]
    fn evaluate_max_equal() {
        let script = ScriptLang::from_representation("08 08 OP_MAX").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context);

        assert!(context.is_valid());
        assert!(context.stack_has_items(1));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(vec![0x08]));
    }

    #[test]
    fn evaluate_within() {
        let script = ScriptLang::from_representation("05 06 03 OP_WITHIN").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context);

        assert!(context.is_valid());
        assert!(context.stack_has_items(1));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(ELEMENT_ONE.to_vec()));
    }

    #[test]
    fn evaluate_within_left() {
        let script = ScriptLang::from_representation("03 06 03 OP_WITHIN").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context);

        assert!(context.is_valid());
        assert!(context.stack_has_items(1));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(ELEMENT_ONE.to_vec()));
    }

    #[test]
    fn evaluate_within_out_left() {
        let script = ScriptLang::from_representation("02 06 03 OP_WITHIN").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context);

        assert!(context.stack_has_items(1));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(ELEMENT_ZERO.to_vec()));
    }

    #[test]
    fn evaluate_within_right() {
        let script = ScriptLang::from_representation("06 06 03 OP_WITHIN").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context);

        assert!(context.stack_has_items(1));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(ELEMENT_ZERO.to_vec()));
    }

    #[test]
    fn evaluate_within_out_right() {
        let script = ScriptLang::from_representation("07 06 03 OP_WITHIN").unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context);

        assert!(context.stack_has_items(1));

        let op = context.stack_pop_as_element().unwrap();
        assert_eq!(op, Token::Element(ELEMENT_ZERO.to_vec()));
    }

    #[test]
    fn evaluate_script_sha1_collision() {
        let c1 = "255044462D312E330A25E2E3CFD30A0A0A312030206F626A0A3C3C2F57696474682032203020522F4865696768742033203020522F547970652034203020522F537562747970652035203020522F46696C7465722036203020522F436F6C6F7253706163652037203020522F4C656E6774682038203020522F42697473506572436F6D706F6E656E7420383E3E0A73747265616D0AFFD8FFFE00245348412D3120697320646561642121212121852FEC092339759C39B1A1C63C4C97E1FFFE017F46DC93A6B67E013B029AAA1DB2560B45CA67D688C7F84B8C4C791FE02B3DF614F86DB1690901C56B45C1530AFEDFB76038E972722FE7AD728F0E4904E046C230570FE9D41398ABE12EF5BC942BE33542A4802D98B5D70F2A332EC37FAC3514E74DDC0F2CC1A874CD0C78305A21566461309789606BD0BF3F98CDA8044629A1";
        let c2 = "255044462d312e330a25e2e3cfd30a0a0a312030206f626a0a3c3c2f57696474682032203020522f4865696768742033203020522f547970652034203020522f537562747970652035203020522f46696c7465722036203020522f436f6c6f7253706163652037203020522f4c656e6774682038203020522f42697473506572436f6d706f6e656e7420383e3e0a73747265616d0affd8fffe00245348412d3120697320646561642121212121852fec092339759c39b1a1c63c4c97e1fffe017346dc9166b67e118f029ab621b2560ff9ca67cca8c7f85ba84c79030c2b3de218f86db3a90901d5df45c14f26fedfb3dc38e96ac22fe7bd728f0e45bce046d23c570feb141398bb552ef5a0a82be331fea48037b8b5d71f0e332edf93ac3500eb4ddc0decc1a864790c782c76215660dd309791d06bd0af3f98cda4bc4629b1";

        let s = format!(
            "{} {} OP_2DUP OP_EQUAL OP_NOT OP_VERIFY OP_SHA1 OP_SWAP OP_SHA1 OP_EQUAL",
            c1, c2
        );
        let script = ScriptLang::from_representation(&s).unwrap();
        let mut context = Context::new(script.tokens(), Integer::from(0));
        let _valid = script.evaluate(&mut context).unwrap();

        assert!(context.is_valid());
        assert!(context.stack_has_items(1));
    }

    // TODO: prepare tests when expected data is not an element

    //
    // elem bigger than 4 bytes
    //
    macro_rules! evaluate_elem_bigger_than_4_bytes {
        ($n:literal, $f:ident) => {
            #[test]
            fn $f() {
                let script = ScriptLang::from_representation($n).unwrap();
                let mut context = Context::new(script.tokens(), Integer::from(0));
                let valid = script.evaluate(&mut context);

                assert_eq!("input_length_too_long", valid.expect_err("Err").to_string());
            }
        };
    }

    evaluate_elem_bigger_than_4_bytes!("FFFFFFFFFF OP_0NOTEQUAL", evaluate_op_0notequal_bigger_than_4_bytes);
    evaluate_elem_bigger_than_4_bytes!("FFFFFFFFFF 00 OP_BOOLAND", evaluate_op_booland_bigger_than_4_bytes_1);
    evaluate_elem_bigger_than_4_bytes!("00 FFFFFFFFFF OP_BOOLAND", evaluate_op_booland_bigger_than_4_bytes_2);
    evaluate_elem_bigger_than_4_bytes!("FFFFFFFFFF 00 OP_BOOLOR", evaluate_op_boolor_bigger_than_4_bytes_1);
    evaluate_elem_bigger_than_4_bytes!("00 FFFFFFFFFF OP_BOOLOR", evaluate_op_boolor_bigger_than_4_bytes_2);
    evaluate_elem_bigger_than_4_bytes!("FFFFFFFFFF OP_ABS", evaluate_op_abs_bigger_than_4_bytes);
    evaluate_elem_bigger_than_4_bytes!("FFFFFFFFFF OP_NEGATE", evaluate_op_negate_bigger_than_4_bytes);
    evaluate_elem_bigger_than_4_bytes!("FFFFFFFFFF OP_1SUB", evaluate_op_1sub_bigger_than_4_bytes);
    evaluate_elem_bigger_than_4_bytes!("FFFFFFFFFF OP_1ADD", evaluate_op_1add_bigger_than_4_bytes);
    evaluate_elem_bigger_than_4_bytes!("FFFFFFFFFF OP_NOT", evaluate_op_not_bigger_than_4_bytes);
    evaluate_elem_bigger_than_4_bytes!("FFFFFFFFFF 00 OP_SUB", evaluate_op_sub_bigger_than_4_bytes_1);
    evaluate_elem_bigger_than_4_bytes!("00 FFFFFFFFFF OP_SUB", evaluate_op_sub_bigger_than_4_bytes_2);
    evaluate_elem_bigger_than_4_bytes!("FFFFFFFFFF 00 OP_ADD", evaluate_op_add_bigger_than_4_bytes_1);
    evaluate_elem_bigger_than_4_bytes!("00 FFFFFFFFFF OP_ADD", evaluate_op_add_bigger_than_4_bytes_2);
    evaluate_elem_bigger_than_4_bytes!("FFFFFFFFFF 00 OP_NUMEQUAL", evaluate_op_numequal_bigger_than_4_bytes_1);
    evaluate_elem_bigger_than_4_bytes!("00 FFFFFFFFFF OP_NUMEQUAL", evaluate_op_numequal_bigger_than_4_bytes_2);
    evaluate_elem_bigger_than_4_bytes!(
        "FFFFFFFFFF 00 OP_NUMNOTEQUAL",
        evaluate_op_numnotequal_bigger_than_4_bytes_1
    );
    evaluate_elem_bigger_than_4_bytes!(
        "00 FFFFFFFFFF OP_NUMNOTEQUAL",
        evaluate_op_numnotequal_bigger_than_4_bytes_2
    );

    //
    // Ignored
    //
    macro_rules! evaluate_op_ignored {
        ($n:literal, $f:ident) => {
            #[test]
            fn $f() {
                let script = ScriptLang::from_representation($n).unwrap();
                let mut context = Context::new(script.tokens(), Integer::from(0));
                let _valid = script.evaluate(&mut context).unwrap();

                assert!(context.stack_has_items(0));
            }
        };
    }

    evaluate_op_ignored!("OP_NOP1", evaluate_op_nop1);
    evaluate_op_ignored!("OP_NOP4", evaluate_op_nop4);
    evaluate_op_ignored!("OP_NOP5", evaluate_op_nop5);
    evaluate_op_ignored!("OP_NOP6", evaluate_op_nop6);
    evaluate_op_ignored!("OP_NOP7", evaluate_op_nop7);
    evaluate_op_ignored!("OP_NOP8", evaluate_op_nop8);
    evaluate_op_ignored!("OP_NOP9", evaluate_op_nop9);
    evaluate_op_ignored!("OP_NOP10", evaluate_op_nop10);

    //
    // Reserved
    //
    macro_rules! evaluate_op_reserved {
        ($n:literal, $f:ident) => {
            #[test]
            fn $f() {
                let script = ScriptLang::from_representation($n).unwrap();
                let mut context = Context::new(script.tokens(), Integer::from(0));
                let valid = script.evaluate(&mut context);

                assert_eq!("exit_by_reserved", valid.expect_err("Err").to_string());
            }
        };
    }

    evaluate_op_reserved!("OP_RESERVED", evaluate_op_reserved);
    evaluate_op_reserved!("OP_VER", evaluate_op_ver);
    evaluate_op_reserved!("OP_VERIF", evaluate_op_verif);
    evaluate_op_reserved!("OP_VERNOTIF", evaluate_op_vernotif);
    evaluate_op_reserved!("OP_RESERVED1", evaluate_op_reserved1);
    evaluate_op_reserved!("OP_RESERVED2", evaluate_op_reserved2);

    //
    // Deprecated
    //
    macro_rules! evaluate_op_deprecated {
        ($n:literal, $f:ident) => {
            #[test]
            fn $f() {
                let script = ScriptLang::from_representation($n).unwrap();
                let mut context = Context::new(script.tokens(), Integer::from(0));
                let valid = script.evaluate(&mut context);

                assert_eq!("deprecated_opcode", valid.expect_err("Err").to_string());
            }
        };
    }

    // evaluate_op_deprecated!("OP_MUL", evaluate_op_mul);
    evaluate_op_deprecated!("OP_CAT", evaluate_op_cat);
    evaluate_op_deprecated!("OP_SUBSTR", evaluate_op_substr);
    evaluate_op_deprecated!("OP_LEFT", evaluate_op_left);
    evaluate_op_deprecated!("OP_RIGHT", evaluate_op_right);
    evaluate_op_deprecated!("OP_INVERT", evaluate_op_invert);
    evaluate_op_deprecated!("OP_AND", evaluate_op_and);
    evaluate_op_deprecated!("OP_OR", evaluate_op_or);
    evaluate_op_deprecated!("OP_XOR", evaluate_op_xor);
    evaluate_op_deprecated!("OP_2MUL", evaluate_op_2mul);
    evaluate_op_deprecated!("OP_2DIV", evaluate_op_2div);
    evaluate_op_deprecated!("OP_DIV", evaluate_op_div);
    evaluate_op_deprecated!("OP_MOD", evaluate_op_mod);
    evaluate_op_deprecated!("OP_LSHIFT", evaluate_op_lshift);
    evaluate_op_deprecated!("OP_RSHIFT", evaluate_op_rshift);

    //
    // Invalid
    //
    macro_rules! evaluate_op_invalid {
        ($n:literal, $f:ident) => {
            #[test]
            fn $f() {
                let script = ScriptLang::from_representation($n).unwrap();
                let mut context = Context::new(script.tokens(), Integer::from(0));
                let valid = script.evaluate(&mut context);

                assert_eq!("invalid_opcode", valid.expect_err("Err").to_string());
            }
        };
    }

    evaluate_op_invalid!("OP_PUBKEY", evaluate_op_pubkey);
    evaluate_op_invalid!("OP_PUBKEYHASH", evaluate_op_pubkeyhash);
    evaluate_op_invalid!("OP_INVALIDOPCODE", evaluate_op_invalidopcode);

    //
    // P2PK
    //
    #[test]
    fn evaluate_p2pk() {
        let signature = "3045022000eff69ef2b1bd93a66ed5219add4fb51e11a840f404876325a1e8ffe0529a2c022100c7207fee197d27c618aea621406f6bf5ef6fca38681d82b2f06fddbdce6feab601";
        let pubkey = "04887387e452b8eacc4acfde10d9aaf7f6d9a0f975aabb10d006e4da568744d06c61de6d95231cd89026e286df3b6ae4a894a3378e393e93a0f45b666329a0ae34";

        let script = ScriptLang::from_representation(&format!("{} {} OP_CHECKSIG", signature, pubkey)).unwrap();
        let z: Integer = Integer::from_hex_str("7C076FF316692A3D7EB3C3BB0F8B1488CF72E1AFCD929E29307032997A838A3D");

        let mut context = Context::new(script.tokens(), z);

        assert!(script.evaluate(&mut context).unwrap());
        assert!(context.is_valid());
    }

    //
    // P2PKH
    //
    #[test]
    fn evaluate_p2pkh_1() {
        let signature = "3045022000eff69ef2b1bd93a66ed5219add4fb51e11a840f404876325a1e8ffe0529a2c022100c7207fee197d27c618aea621406f6bf5ef6fca38681d82b2f06fddbdce6feab601";
        let pubkey = "04887387e452b8eacc4acfde10d9aaf7f6d9a0f975aabb10d006e4da568744d06c61de6d95231cd89026e286df3b6ae4a894a3378e393e93a0f45b666329a0ae34";
        let hash = "fb6c931433c83e8bb5a4c6588c7fc24c08dac6e3";

        let script = ScriptLang::from_representation(&format!(
            "{} {} OP_DUP OP_HASH160 {} OP_EQUALVERIFY OP_CHECKSIG",
            signature, pubkey, hash
        ))
        .unwrap();
        let z: Integer = Integer::from_hex_str("7C076FF316692A3D7EB3C3BB0F8B1488CF72E1AFCD929E29307032997A838A3D");

        let mut context = Context::new(script.tokens(), z);

        assert!(script.evaluate(&mut context).unwrap());
        assert!(context.is_valid());
    }

    #[test]
    fn evaluate_p2pkh_2() {
        // generate a key
        let key = new(Network::Testnet);
        let pub_key = key.pubkey;
        let pub_key_hash = hash160(&pub_key);

        let message_bytes = vec![0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF]; //  this represents the serialized transaction
        let message_hash = hash160(&message_bytes);
        let z = Integer::from_digits(&message_hash, Order::Msf);

        // with the key, sign the message
        let signature = key.key.sign(z.clone());
        let mut signature_der = signature.der();
        signature_der.push(SigHash::All as u8);

        // prepare the script (ScriptSig + CheckSig script)
        let mut script = standard::p2pkh_script(&pub_key_hash);
        let script_sig = vec![Token::Element(signature_der), Token::Element(pub_key)];
        script = script.prepend(script_sig);

        // evaluate
        let mut context = Context::new(script.tokens(), z);

        assert!(script.evaluate(&mut context).unwrap());
        assert!(context.is_valid());
    }

    //
    // P2MS
    //
    #[test]
    fn evaluate_p2ms_1_to_1() {
        // generate a key
        let key = new(Network::Testnet);
        let pub_key = key.pubkey;

        let message_bytes = vec![0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF]; //  this represents the serialized transaction
        let message_hash = hash160(&message_bytes);
        let z = Integer::from_digits(&message_hash, Order::Msf);

        // with the key, sign the message
        let signature = key.key.sign(z.clone());
        let mut signature_der = signature.der();
        signature_der.push(SigHash::All as u8);

        // prepare the script (ScriptSig + CheckSig script)
        let mut script = standard::p2ms_script(1, &vec![pub_key.as_slice()]);
        let script_sig = vec![Token::Command(OP_0), Token::Element(signature_der)];
        script = script.prepend(script_sig);

        // evaluate
        let mut context = Context::new(script.tokens(), z);

        assert!(script.evaluate(&mut context).unwrap());
        assert!(context.is_valid());
    }

    #[test]
    fn evaluate_p2ms_1_to_2() {
        // generate two keys
        let key1 = new(Network::Testnet);
        let pub_key1 = key1.pubkey;

        let key2 = new(Network::Testnet);
        let pub_key2 = key2.pubkey;

        let message_bytes = vec![0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF]; //  this represents the serialized transaction
        let message_hash = hash160(&message_bytes);
        let z = Integer::from_digits(&message_hash, Order::Msf);

        // with the key, sign the message
        let signature = key1.key.sign(z.clone());
        let mut signature_der = signature.der();
        signature_der.push(SigHash::All as u8);

        // prepare the script (ScriptSig + CheckSig script)
        let mut script = standard::p2ms_script(1, &vec![pub_key1.as_slice(), pub_key2.as_slice()]);
        let script_sig = vec![Token::Command(OP_0), Token::Element(signature_der)];
        script = script.prepend(script_sig);

        // evaluate
        let mut context = Context::new(script.tokens(), z);

        assert!(script.evaluate(&mut context).unwrap());
        assert!(context.is_valid());
    }

    #[test]
    fn evaluate_p2ms_2_to_2() {
        // generate two keys
        let key1 = new(Network::Testnet);
        let pub_key1 = key1.pubkey;

        let key2 = new(Network::Testnet);
        let pub_key2 = key2.pubkey;

        let message_bytes = vec![0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF]; //  this represents the serialized transaction
        let message_hash = hash160(&message_bytes);
        let z = Integer::from_digits(&message_hash, Order::Msf);

        // with the keys, sign the message
        let signature1 = key1.key.sign(z.clone());
        let mut signature1_der = signature1.der();
        signature1_der.push(SigHash::All as u8);

        let signature2 = key2.key.sign(z.clone());
        let mut signature2_der = signature2.der();
        signature2_der.push(SigHash::All as u8);

        // prepare the script (ScriptSig + CheckSig script)
        let mut script = standard::p2ms_script(2, &vec![pub_key1.as_slice(), pub_key2.as_slice()]);
        let script_sig = vec![
            Token::Command(OP_0),
            Token::Element(signature1_der),
            Token::Element(signature2_der),
        ];
        script = script.prepend(script_sig);

        // evaluate
        let mut context = Context::new(script.tokens(), z);

        assert!(script.evaluate(&mut context).unwrap());
        assert!(context.is_valid());
    }

    #[test]
    fn evaluate_p2ms_2_to_2_but_inverted_pubkeys() {
        // generate two keys
        let key1 = new(Network::Testnet);
        let pub_key1 = key1.pubkey;

        let key2 = new(Network::Testnet);
        let pub_key2 = key2.pubkey;

        let message_bytes = vec![0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF]; //  this represents the serialized transaction
        let message_hash = hash160(&message_bytes);
        let z = Integer::from_digits(&message_hash, Order::Msf);

        // with the keys, sign the message
        let signature1 = key1.key.sign(z.clone());
        let mut signature1_der = signature1.der();
        signature1_der.push(SigHash::All as u8);

        let signature2 = key2.key.sign(z.clone());
        let mut signature2_der = signature2.der();
        signature2_der.push(SigHash::All as u8);

        // prepare the script (ScriptSig + CheckSig script) but with (invalid) inverted pubkeys
        let mut script = standard::p2ms_script(2, &vec![pub_key2.as_slice(), pub_key1.as_slice()]);
        let script_sig = vec![
            Token::Command(OP_0),
            Token::Element(signature1_der),
            Token::Element(signature2_der),
        ];
        script = script.prepend(script_sig);

        // evaluate
        let mut context = Context::new(script.tokens(), z);

        assert!(!script.evaluate(&mut context).unwrap());
        assert!(!context.is_valid());
    }
}
