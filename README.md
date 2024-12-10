## RESP2 Parser

This project is a [RESP2 (REdis Serialization Protocol Version 2)](https://github.com/redis/redis-specifications/blob/master/protocol/RESP2.md) Parser implemented in Rust. 
It demonstrates how to parse and handle the different data types in RESP2, including simple strings, errors, integers, bulk strings, and arrays. 
The parser supports recursive parsing, allowing it to handle nested arrays and mixed data types. 
It is built using the `nom` library, a nice library for constructing parsers in Rust. 
The code is designed for educational purposes. 
Comprehensive test cases ensure correctness for a variety of valid and invalid inputs, making the project a decent learning resource.