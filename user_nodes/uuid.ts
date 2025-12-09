function spec() {
  return {
    name: "UUID",
    title: "Generate UUID",
    category: "Data",
    description: "Generates a random UUID v4 using crypto.randomUUID()",
    inputs: [],
    outputs: [
      { name: "uuid", type: "string" }
    ]
  };
}

function execute(inputs) {
  return {
    uuid: crypto.randomUUID()
  };
}
