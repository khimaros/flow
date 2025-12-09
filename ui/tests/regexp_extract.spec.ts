import { test, expect } from "@playwright/test";
import { FlowPage } from "./pages/FlowPage";

test.describe("RegexpExtract Node", () => {
  let flowPage: FlowPage;

  test.beforeEach(async ({ page }) => {
    flowPage = new FlowPage(page, { debug: true });
    await flowPage.goto();
  });

  test("load and execute regexp-extract workflow", async () => {
    await flowPage.loadWorkflowAndVerify("regexp-extract", ["Regexp Extract"]);
    await flowPage.runWorkflowAndExpectComplete();
  });

  test("add RegexpExtract node and execute with inputs", async () => {
    await flowPage.createNewWorkflow({ ignoreUnsaved: true });
    await flowPage.addNode("Regexp Extract");
    await flowPage.expectNodeCount(1);

    // fill text and pattern inputs (textarea + text input)
    await flowPage.fillNodeInputs("Regexp Extract", [
      "hello world hello",
      "hello",
    ]);

    await flowPage.runWorkflowAndExpectComplete();
    await flowPage.expectNodeFinished("Regexp Extract");
  });

  test("RegexpExtract node has BooleanSelect for case_sensitive", async () => {
    await flowPage.createNewWorkflow({ ignoreUnsaved: true });
    await flowPage.addNode("Regexp Extract");

    const node = flowPage.getNode("Regexp Extract");
    const selectInputs = node.locator("select");
    await expect(selectInputs.first()).toBeVisible();

    // verify the boolean select has true/false options
    const options = selectInputs.first().locator("option");
    await expect(options).toHaveCount(2);
    await expect(options.first()).toHaveText("true");
    await expect(options.last()).toHaveText("false");
  });
});
