use domain::tag::ValueParser;
use infrastructure::pipeline::ScaleParser;

#[test] // parses_simple_numeric_weight
fn parses_simple_numeric_weight() {
    let parser = ScaleParser::new();
    let raw = "   5.00kg";
    let result = parser.parse(raw).expect("Should parse successfully");
    assert_eq!(result["value"], 5.00);
    assert_eq!(result["unit"], "kg");
}

#[test] // parses_negative_weight
fn parses_negative_weight() {
    let parser = ScaleParser::new();
    let raw = "-12.45 kg";
    let result = parser.parse(raw).expect("Should parse successfully");
    assert_eq!(result["value"], -12.45);
    assert_eq!(result["unit"], "kg");
}

#[test] // parses_weight_with_spaces_in_unit
fn parses_weight_with_spaces_in_unit() {
    let parser = ScaleParser::new();
    let raw = "   0.532   g";
    let result = parser.parse(raw).expect("Should parse successfully");
    assert_eq!(result["value"], 0.532);
    assert_eq!(result["unit"], "g");
}

#[test] // parses_weight_with_trailing_spaces
fn parses_weight_with_trailing_spaces() {
    let parser = ScaleParser::new();
    let raw = "   5.00kg   ";
    let result = parser.parse(raw).expect("Should parse successfully");
    assert_eq!(result["value"], 5.00);
    assert_eq!(result["unit"], "kg");
}

#[test] // parses_weight_with_leading_spaces
fn parses_weight_with_leading_spaces() {
    let parser = ScaleParser::new();
    let raw = " -  5.00kg";
    let result = parser.parse(raw).expect("Should parse successfully");
    assert_eq!(result["value"], -5.00);
    assert_eq!(result["unit"], "kg");
}

#[test] // parses_rs232_prefixed_message
fn parses_rs232_prefixed_message() {
    let parser = ScaleParser::new();
    let raw = "ST,GS,  5.00kg";
    let result = parser.parse(raw).expect("Should parse successfully");
    assert_eq!(result["value"], 5.00);
    assert_eq!(result["unit"], "kg");
}

#[test] // parses_message_with_comma_decimal
fn parses_message_with_comma_decimal() {
    let parser = ScaleParser::new();
    let raw = "5,30kg";
    let result = parser.parse(raw).expect("Should parse successfully");
    assert_eq!(result["value"], 5.30);
    assert_eq!(result["unit"], "kg");
}

#[test] // parses_message_with_stability_prefix
fn parses_message_with_stability_prefix() {
    let parser = ScaleParser::new();

    let stable = "ST  5.0kg";
    let unstable = "US  5.0kg";

    let st = parser.parse(stable).expect("Should parse ST");
    let us = parser.parse(unstable).expect("Should parse US");

    assert_eq!(st["value"], 5.0);
    assert_eq!(st["unit"], "kg");

    assert_eq!(us["value"], 5.0);
    assert_eq!(us["unit"], "kg");
}

#[test] // fails_with_invalid_message
fn fails_with_invalid_message() {
    let parser = ScaleParser::new();
    let invalids = vec!["ERROR", "---", "?? 12", "BAD DATA", "12"]; // "12" is invalid because no unit

    for raw in invalids {
        let result = parser.parse(raw);
        assert!(result.is_err(), "Should fail on: {}", raw);
    }
}

#[test]
fn parses_attached_unit() {
    let parser = ScaleParser::new();

    // "1.1g"
    let result = parser.parse("1.1g").expect("Should parse 1.1g");
    assert_eq!(result["value"], 1.1);
    assert_eq!(result["unit"], "g");

    // "123g"
    let result = parser.parse("123g").expect("Should parse 123g");
    assert_eq!(result["value"], 123.0);
    assert_eq!(result["unit"], "g");
}

#[test]
fn test_range_validator_with_composite_value() {
    use domain::tag::ValueValidator;
    use infrastructure::pipeline::RangeValidator;
    use serde_json::json;

    let validator = RangeValidator::new(Some(0.0), Some(100.0));
    let val = json!({
        "value": 50.0,
        "unit": "kg"
    });

    // Currently fails because it's an object, not a number
    // We want this to pass by extracting "value"
    let result = validator.validate(&val);
    assert!(
        result.is_ok(),
        "Should validate composite value: {:?}",
        result.err()
    );
}
