use nom::branch::alt;
use nom::multi::many1;
use nom::error::Error;
use nom::{Finish, IResult};
use nom::bytes::complete::{is_a, take, take_while1};
use nom::sequence::{preceded, terminated, tuple};
use nom::combinator::{map, map_res, opt, recognize};
use nom::character::complete::{char, crlf, digit1, none_of, one_of};


#[derive(Debug, PartialEq)]
pub enum Resp2Type {
    SimpleString(String),
    SimpleError(Resp2SimpleError),
    Integer(i64),
    BulkString(String),
    NullBulkString,
    Array(Vec<Resp2Type>),
    NullArray
}


#[derive(Debug, PartialEq)]
struct Resp2SimpleError {
    kind: String,
    message: String
}


pub struct Resp2Codec;

impl Resp2Codec {
    pub fn parse(input: &str) -> Result<Resp2Type, Error<&str>> {
        let parsing_result = Self::parse_internal(input);

        match parsing_result.finish() {
            Ok((_, resp_type)) => Ok(resp_type),
            Err(error) => Err(error)
        }
    }


    fn parse_internal(input: &str) -> IResult<&str, Resp2Type> {
        let (input, first_character) = one_of("+-:$*")(input)?;

        match first_character {
            '+' => Self::parse_simple_string(input),
            '-' => Self::parse_simple_error(input),
            ':' => Self::parse_int(input),
            '$' => Self::parse_bulk_string(input),
            '*' => Self::parse_array(input),
            _ => unreachable!()
        }
    }


    fn parse_simple_string(input: &str) -> IResult<&str, Resp2Type> {
        map(
            tuple((many1(none_of("\r\n")), char('\r'), char('\n'))),
            |(character_vector, _, _)| Resp2Type::SimpleString(character_vector.into_iter().collect::<String>())
        )(input)
    }


    fn parse_simple_error(input: &str) -> IResult<&str, Resp2Type> {
        map(
            tuple(
            (take_while1::<_, &str, _>(|c| c.is_ascii_uppercase()), preceded(is_a(" \n"), Self::parse_simple_string))
            ),
            |(kind, simple_string)| Resp2Type::SimpleError(Resp2SimpleError {
                kind: kind.to_string(),
                message: match simple_string {
                    Resp2Type::SimpleString(msg) => msg,
                    _ => panic!("Expected Resp2Type::SimpleString")
                }
            })
        )(input)
    }


    fn parse_int(input: &str) -> IResult<&str, Resp2Type> {
        let result = map_res(
            terminated(
                recognize(
                    tuple(
                        (opt(alt((char::<&str, _>('+'), char::<&str, _>('-')))), digit1)
                    ),
                ),
                crlf
            ),
            |digits| digits.parse::<i64>()
        )(input);

        result.map(|(rest, number)| (rest, Resp2Type::Integer(number)))
    }


    fn parse_bulk_string(input: &str) -> IResult<&str, Resp2Type> {
        let (input, length) = terminated(
            map_res(
                recognize(tuple((opt(alt((char('+'), char('-')))), digit1))),
                |digits: &str| digits.parse::<isize>()
            ),
            crlf
        )(input)?;

        if length == -1 {
            return Ok((input, Resp2Type::NullBulkString));
        }

        let length = length as usize;
        let (input, data) = take(length)(input)?;
        let (input, _) = crlf(input)?;

        Ok((input, Resp2Type::BulkString(data.to_string())))
    }


    fn parse_array(input: &str) -> IResult<&str, Resp2Type> {
        let (input, length) = terminated(
            map_res(
                recognize(tuple((opt(alt((char('+'), char('-')))), digit1))),
                |digits: &str| digits.parse::<isize>()
            ),
            crlf
        )(input)?;

        if length == -1 {
            return Ok((input, Resp2Type::NullArray))
        }

        let length = length as usize;
        let mut input = input;
        let mut elements = Vec::new();

        for _ in 0..length {
            let (new_input, element) = Self::parse_internal(input)?;
            input = new_input;
            elements.push(element);
        }

        Ok((input, Resp2Type::Array(elements)))
    }
}


#[cfg(test)]
mod tests {
    use crate::{Resp2Codec, Resp2SimpleError, Resp2Type};


    #[test]
    fn test_valid_simple_strings() {
        assert_eq!(
            Resp2Codec::parse("+OK\r\n"),
            Ok(Resp2Type::SimpleString("OK".to_string()))
        );

        assert_eq!(
            Resp2Codec::parse("+Hello, World! 123 @#$%\r\n"),
            Ok(Resp2Type::SimpleString("Hello, World! 123 @#$%".to_string()))
        );

        assert_eq!(
            Resp2Codec::parse("+Pong\r\nREMAINING"),
            Ok(Resp2Type::SimpleString("Pong".to_string()))
        );
    }


    #[test]
    fn test_invalid_simple_strings() {
        let result = Resp2Codec::parse("OK\r\n");
        assert!(result.is_err());

        let result = Resp2Codec::parse("+OK\n");
        assert!(result.is_err());

        let result = Resp2Codec::parse("+OK\r");
        assert!(result.is_err());

        let result = Resp2Codec::parse("+OK");
        assert!(result.is_err());

        let result = Resp2Codec::parse("+OK\rSTILL_HERE\r\n");
        assert!(result.is_err());

        let result = Resp2Codec::parse("+OK\nSTILL_HERE\r\n");
        assert!(result.is_err());

        let result = Resp2Codec::parse("+\r\n");
        assert!(result.is_err());
    }


    #[test]
    fn test_valid_errors() {
        assert_eq!(
            Resp2Codec::parse("-ERR unknown command\r\n"),
            Ok(Resp2Type::SimpleError(Resp2SimpleError{ kind: "ERR".to_string(), message: "unknown command".to_string() }))
        );

        assert_eq!(
            Resp2Codec::parse("-WRONGTYPE Operation against a key holding the wrong kind of value\r\n"),
            Ok(Resp2Type::SimpleError(Resp2SimpleError{ kind: "WRONGTYPE".to_string(), message: "Operation against a key holding the wrong kind of value".to_string() }))
        );

        assert_eq!(
            Resp2Codec::parse("-ERR Test! 123 @#$%\r\n"),
            Ok(Resp2Type::SimpleError(Resp2SimpleError{ kind: "ERR".to_string(), message: "Test! 123 @#$%".to_string() }))
        );

        assert_eq!(
            Resp2Codec::parse("-ERR\nUnknown error\r\n"),
            Ok(Resp2Type::SimpleError(Resp2SimpleError{ kind: "ERR".to_string(), message: "Unknown error".to_string() }))
        );

        assert_eq!(
            Resp2Codec::parse("-ERR  \n  Unknown error\r\n"),
            Ok(Resp2Type::SimpleError(Resp2SimpleError{ kind: "ERR".to_string(), message: "Unknown error".to_string() }))
        );

        assert_eq!(
            Resp2Codec::parse("-ERR  \n\n \n Unknown error\r\n"),
            Ok(Resp2Type::SimpleError(Resp2SimpleError{ kind: "ERR".to_string(), message: "Unknown error".to_string() }))
        );
    }


    #[test]
    fn test_invalid_errors() {
        let result = Resp2Codec::parse("ERR unknown command\r\n");
        assert!(result.is_err());

        let result = Resp2Codec::parse("-ERR unknown command");
        assert!(result.is_err());

        let result = Resp2Codec::parse("-ERR unknown command\n");
        assert!(result.is_err());

        let result = Resp2Codec::parse("-ERR unknown command\r");
        assert!(result.is_err());

        let result = Resp2Codec::parse("-ERR unknown\rSTILL_HERE\r\n");
        assert!(result.is_err());

        let result = Resp2Codec::parse("-ERR unknown\nSTILL_HERE\r\n");
        assert!(result.is_err());

        let result = Resp2Codec::parse("-\r\n");
        assert!(result.is_err());

        let result = Resp2Codec::parse("-err unknown command\r\n");
        assert!(result.is_err());

        let result = Resp2Codec::parse("-ErR unknown command\r\n");
        assert!(result.is_err());
    }


    #[test]
    fn test_valid_integers() {
        assert_eq!(
            Resp2Codec::parse(":0\r\n"),
            Ok(Resp2Type::Integer(0))
        );
        assert_eq!(
            Resp2Codec::parse(":1000\r\n"),
            Ok(Resp2Type::Integer(1000))
        );

        assert_eq!(
            Resp2Codec::parse(":+1234567890\r\n"),
            Ok(Resp2Type::Integer(1234567890))
        );
        assert_eq!(
            Resp2Codec::parse(":+0\r\n"),
            Ok(Resp2Type::Integer(0))
        );

        assert_eq!(
            Resp2Codec::parse(":-1234567890\r\n"),
            Ok(Resp2Type::Integer(-1234567890))
        );
        assert_eq!(
            Resp2Codec::parse(":-0\r\n"),
            Ok(Resp2Type::Integer(0))
        );

        assert_eq!(
            Resp2Codec::parse(":9223372036854775807\r\n"),
            Ok(Resp2Type::Integer(i64::MAX))
        );
        assert_eq!(
            Resp2Codec::parse(":-9223372036854775808\r\n"),
            Ok(Resp2Type::Integer(i64::MIN))
        );

        assert_eq!(
            Resp2Codec::parse(":000042\r\n"),
            Ok(Resp2Type::Integer(42))
        );
        assert_eq!(
            Resp2Codec::parse(":+000042\r\n"),
            Ok(Resp2Type::Integer(42))
        );
        assert_eq!(
            Resp2Codec::parse(":-000042\r\n"),
            Ok(Resp2Type::Integer(-42))
        );
    }


    #[test]
    fn test_invalid_integers() {
        let result = Resp2Codec::parse("42\r\n");
        assert!(result.is_err());

        let result = Resp2Codec::parse(":42");
        assert!(result.is_err());
        let result = Resp2Codec::parse(":42\n");
        assert!(result.is_err());
        let result = Resp2Codec::parse(":42\r");
        assert!(result.is_err());

        let result = Resp2Codec::parse(":4a2\r\n");
        assert!(result.is_err());
        let result = Resp2Codec::parse(":42a\r\n");
        assert!(result.is_err());
        let result = Resp2Codec::parse(": 42\r\n");
        assert!(result.is_err());
        let result = Resp2Codec::parse(":42 \r\n");
        assert!(result.is_err());

        let result = Resp2Codec::parse(":+-42\r\n");
        assert!(result.is_err());
        let result = Resp2Codec::parse(":--42\r\n");
        assert!(result.is_err());
        let result = Resp2Codec::parse(":++42\r\n");
        assert!(result.is_err());


        let result = Resp2Codec::parse(":\r\n");
        assert!(result.is_err());
        let result = Resp2Codec::parse(":+\r\n");
        assert!(result.is_err());
        let result = Resp2Codec::parse(":-\r\n");
        assert!(result.is_err());


        let result = Resp2Codec::parse(":42\rSTILL_HERE\r\n");
        assert!(result.is_err());
        let result = Resp2Codec::parse(":42\nSTILL_HERE\r\n");
        assert!(result.is_err());


        let result = Resp2Codec::parse(":9223372036854775808\r\n");
        assert!(result.is_err());
        let result = Resp2Codec::parse(":-9223372036854775809\r\n");
        assert!(result.is_err());
        let result = Resp2Codec::parse(":18446744073709551616\r\n");
        assert!(result.is_err());


        let result = Resp2Codec::parse(":４２\r\n");  // Full-width digits
        assert!(result.is_err());
        let result = Resp2Codec::parse(":42\x00\r\n");
        assert!(result.is_err());

        let result = Resp2Codec::parse(":42.0\r\n");
        assert!(result.is_err());
        let result = Resp2Codec::parse(":4.2e1\r\n");
        assert!(result.is_err());
        let result = Resp2Codec::parse(":4e2\r\n");
        assert!(result.is_err());
    }

    #[test]
    fn test_valid_bulk_strings() {
        assert_eq!(
            Resp2Codec::parse("$5\r\nhello\r\n"),
            Ok(Resp2Type::BulkString("hello".to_string()))
        );
        assert_eq!(
            Resp2Codec::parse("$0\r\n\r\n"),
            Ok(Resp2Type::BulkString("".to_string()))
        );

        assert_eq!(
            Resp2Codec::parse("$-1\r\n"),
            Ok(Resp2Type::NullBulkString)
        );

        assert_eq!(
            Resp2Codec::parse("$7\r\n!@#$%^&\r\n"),
            Ok(Resp2Type::BulkString("!@#$%^&".to_string()))
        );

        assert_eq!(
            Resp2Codec::parse("$4\r\n\x00\x01\x02\x03\r\n"),
            Ok(Resp2Type::BulkString("\x00\x01\x02\x03".to_string()))
        );
    }

    #[test]
    fn test_invalid_bulk_strings() {
        assert!(Resp2Codec::parse("5\r\nhello\r\n").is_err());
        assert!(Resp2Codec::parse("$\r\nhello\r\n").is_err());

        assert!(Resp2Codec::parse("$\r\n").is_err());

        assert!(Resp2Codec::parse("$abc\r\nhello\r\n").is_err());
        assert!(Resp2Codec::parse("$5x\r\nhello\r\n").is_err());

        assert!(Resp2Codec::parse("$-2\r\nhello\r\n").is_err());

        assert!(Resp2Codec::parse("$5hello\r\n").is_err());
        assert!(Resp2Codec::parse("$5\nhello\r\n").is_err());
        assert!(Resp2Codec::parse("$5\rhello\r\n").is_err());

        assert!(Resp2Codec::parse("$5\r\nhe").is_err());
        assert!(Resp2Codec::parse("$5\r\nhello").is_err());

        assert!(Resp2Codec::parse("$5\r\nhello").is_err());

        assert!(Resp2Codec::parse("$3\r\nhello\r\n").is_err());

        assert!(Resp2Codec::parse("$100\r\nhello\r\n").is_err());

        assert!(Resp2Codec::parse("$5\r\nhello\n\r\n").is_err());
        assert!(Resp2Codec::parse("$5\r\nhello\r").is_err());
    }


    #[test]
    fn test_valid_simple_arrays() {
        assert_eq!(
            Resp2Codec::parse("*3\r\n:1\r\n:2\r\n:3\r\n"),
            Ok(Resp2Type::Array(vec![
                Resp2Type::Integer(1),
                Resp2Type::Integer(2),
                Resp2Type::Integer(3)
            ]))
        );

        assert_eq!(
            Resp2Codec::parse("*2\r\n$5\r\nhello\r\n$5\r\nworld\r\n"),
            Ok(Resp2Type::Array(vec![
                Resp2Type::BulkString("hello".to_string()),
                Resp2Type::BulkString("world".to_string())
            ]))
        );

        assert_eq!(
            Resp2Codec::parse("*0\r\n"),
            Ok(Resp2Type::Array(vec![]))
        );

        assert_eq!(
            Resp2Codec::parse("*1\r\n$5\r\nhello\r\n"),
            Ok(Resp2Type::Array(vec![Resp2Type::BulkString("hello".to_string())]))
        );
    }


    #[test]
    fn test_valid_null_arrays() {
        assert_eq!(
            Resp2Codec::parse("*-1\r\n"),
            Ok(Resp2Type::NullArray)
        );

        assert_eq!(
            Resp2Codec::parse("*1\r\n$-1\r\n"),
            Ok(Resp2Type::Array(vec![Resp2Type::NullBulkString]))
        );
    }


    #[test]
    fn test_valid_nested_arrays() {
        assert_eq!(
            Resp2Codec::parse("*2\r\n*3\r\n:1\r\n:2\r\n:3\r\n*2\r\n+Hello\r\n-ERROR The error message goes here\r\n"),
            Ok(Resp2Type::Array(vec![
                Resp2Type::Array(vec![
                    Resp2Type::Integer(1),
                    Resp2Type::Integer(2),
                    Resp2Type::Integer(3)
                ]),
                Resp2Type::Array(vec![
                    Resp2Type::SimpleString("Hello".to_string()),
                    Resp2Type::SimpleError(Resp2SimpleError {
                        kind: "ERROR".to_string(),
                        message: "The error message goes here".to_string()
                    })
                ])
            ]))
        );

        assert_eq!(
            Resp2Codec::parse("*1\r\n*2\r\n:10\r\n:20\r\n"),
            Ok(Resp2Type::Array(vec![Resp2Type::Array(vec![
                Resp2Type::Integer(10),
                Resp2Type::Integer(20)
            ])]))
        );

        assert_eq!(
            Resp2Codec::parse("*2\r\n*2\r\n*1\r\n:1\r\n*1\r\n:2\r\n*1\r\n:3\r\n"),
            Ok(Resp2Type::Array(vec![
                Resp2Type::Array(vec![
                    Resp2Type::Array(vec![Resp2Type::Integer(1)]),
                    Resp2Type::Array(vec![Resp2Type::Integer(2)])
                ]),
                Resp2Type::Array(vec![Resp2Type::Integer(3)])
            ]))
        );
    }

    #[test]
    fn test_valid_mixed_arrays() {
        assert_eq!(
            Resp2Codec::parse("*4\r\n+Simple\r\n:42\r\n$5\r\nhello\r\n$-1\r\n"),
            Ok(Resp2Type::Array(vec![
                Resp2Type::SimpleString("Simple".to_string()),
                Resp2Type::Integer(42),
                Resp2Type::BulkString("hello".to_string()),
                Resp2Type::NullBulkString
            ]))
        );
    }

    #[test]
    fn test_invalid_array_formats() {
        let result = Resp2Codec::parse("*3\r\n:1\r\n:2");
        assert!(result.is_err());

        let result = Resp2Codec::parse("*2\r\n$5\r\nhello");
        assert!(result.is_err());

        let result = Resp2Codec::parse("*-2\r\n");
        assert!(result.is_err());

        let result = Resp2Codec::parse("*3\r\n:1\r\n:2\r\n*");
        assert!(result.is_err());

        let result = Resp2Codec::parse("*3\r\n:1\r\n$-1\r\n");
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_nested_arrays() {
        let result = Resp2Codec::parse("*2\r\n*3\r\n:1\r\n:2\r\n:3\r\n*");
        assert!(result.is_err());

        let result = Resp2Codec::parse("*1\r\n*2\r\n:10\r\n");
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_mixed_arrays() {
        let result = Resp2Codec::parse("*4\r\n+Simple\r\n42\r\n$5\r\nhello\r\n$-1\r\n");
        assert!(result.is_err());

        let result = Resp2Codec::parse("*3\r\n+Simple\r\n$-2\r\n:42\r\n");
        assert!(result.is_err());
    }
}
