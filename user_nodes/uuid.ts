function spec() {
  return {
    name: "UUID",
    title: "Generate UUID",
    category: "Data",
    description: "Generate a random UUID v4",
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
