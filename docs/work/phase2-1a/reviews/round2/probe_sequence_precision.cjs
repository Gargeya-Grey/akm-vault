const wire = '{"brokenAt":9007199254740993}';
const result = JSON.parse(wire);
console.log(JSON.stringify({ wire, displayed: String(result.brokenAt), exact: String(result.brokenAt) === "9007199254740993" }));
