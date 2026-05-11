import { test, expect } from "@playwright/test";
import { FlowPage } from "./pages/FlowPage";

test.describe("Node Execution and Highlighting", () => {
  let flowPage: FlowPage;

  test.beforeEach(async ({ page }) => {
    flowPage = new FlowPage(page, { debug: true });
    await flowPage.goto();
    await flowPage.createNewWorkflow({ ignoreUnsaved: true });
  });

  test("node shows running state during execution", async () => {
    await flowPage.addAndRunNode("Echo", "test message");

    await flowPage.expectNodeFinished("Echo");
    await flowPage.expectNodeNotRunning("Echo");
  });

  test("chained nodes execute in order and show proper status", async () => {
    const { sourceId, targetId } = await flowPage.createAndConnectNodes(
      "Echo",
      "Echo",
    );

    await flowPage.fillNodeInputById(sourceId, "hello from first", 0);

    await flowPage.runWorkflow();

    await expect(
      flowPage.getNodeById(sourceId).locator(".node-just-finished"),
    ).toBeVisible({ timeout: 5000 });
    await expect(
      flowPage.getNodeById(targetId).locator(".node-just-finished"),
    ).toBeVisible({ timeout: 5000 });
  });

  test("execution queue shows completed jobs", async () => {
    await flowPage.addNode("Echo");
    await flowPage.fillNodeInput("Echo", "queue test");

    await flowPage.runWorkflowAndExpectComplete();

    await flowPage.selectSidebarTab("Queue");
    await flowPage.expectQueueHasJob("completed");
  });

  test("cached nodes show amber highlight instead of green", async () => {
    await flowPage.addAndRunNode("Echo", "cache test message");

    await flowPage.expectNodeFinished("Echo");
    await flowPage.expectNodeNotRunning("Echo");

    // wait for the first run's justFinished glow to clear (1500ms timer)
    // so its timeout doesn't race with the cached run's highlight
    const node = flowPage.getNode("Echo");
    await expect(node.locator(".node-just-finished")).not.toBeVisible({
      timeout: 5000,
    });

    // start watching for cached state before triggering the run,
    // since the cached glow is transient (1500ms) and a fast response
    // can expire before the assertion starts
    const cachedPromise = flowPage.expectNodeCachedColor("Echo");
    await flowPage.runNode("Echo");
    await cachedPromise;
  });

  test("node running state is cleared after completion", async () => {
    await flowPage.addAndRunNode("Echo", "running state test");

    await flowPage.expectNodeFinished("Echo");
    await flowPage.expectNodeNotRunning("Echo");
  });

  test("streaming updates downstream display node", async () => {
    await flowPage.loadWorkflowAndVerify("shell-stream", [
      "Shell Command",
      "Echo",
      "Display Markdown",
    ]);

    await flowPage.runWorkflowAndExpectComplete(true);

    const markdownBody = flowPage
      .getNodeById("displaymarkdown_81d234a5")
      .locator(".markdown-body");
    await expect(markdownBody).toContainText("hello", { timeout: 10000 });
    await expect(markdownBody).toContainText("stream", { timeout: 10000 });
  });

  test("streaming updates downstream Echo input in realtime", async () => {
    await flowPage.loadWorkflowAndVerify("shell-stream", [
      "Shell Command",
      "Echo",
      "Display Markdown",
    ]);

    await flowPage.runWorkflowAndExpectComplete(true);

    const echoInput = flowPage
      .getNodeById("echo_ff24a910")
      .locator("textarea")
      .first();
    await expect(echoInput).toHaveValue(/hello/, { timeout: 10000 });
    await expect(echoInput).toHaveValue(/stream/, { timeout: 10000 });
  });

  test("streaming propagates through Echo to DisplayMarkdown", async () => {
    await flowPage.loadWorkflowAndVerify("shell-stream", [
      "Shell Command",
      "Echo",
      "Display Markdown",
    ]);

    await flowPage.runWorkflowAndExpectComplete(true);

    // verify the full chain: Shell Command -> Echo -> Display Markdown
    const echoInput = flowPage
      .getNodeById("echo_ff24a910")
      .locator("textarea")
      .first();
    const markdownBody = flowPage
      .getNodeById("displaymarkdown_81d234a5")
      .locator(".markdown-body");

    await expect(echoInput).toHaveValue(/hello/, { timeout: 10000 });
    await expect(markdownBody).toContainText("hello", { timeout: 10000 });
    await expect(echoInput).toHaveValue(/stream/, { timeout: 10000 });
    await expect(markdownBody).toContainText("stream", { timeout: 10000 });
  });

  test("force-running one node preserves in-progress state of another", async () => {
    // start a slow node so it's still running when we queue another job
    await flowPage.addNode("Shell Command");
    const shellNode = flowPage.getNode("Shell Command");
    // command field
    await shellNode.locator('input[type="text"]').first().fill("sleep");
    // args field (list editor) — type into "add item..." and Enter to append
    const addItem = shellNode.locator('input[placeholder="add item..."]');
    await addItem.fill("10");
    await addItem.press("Enter");
    await flowPage.runNode("Shell Command");

    const shell = flowPage.getNode("Shell Command");
    await expect(shell.locator(".node-running")).toBeVisible({ timeout: 5000 });

    // add an independent node and force-run it (shift-click the play button)
    await flowPage.addNode("Echo");
    await flowPage.fillNodeInput("Echo", "quick");
    const runButton = flowPage.getNode("Echo").locator('[title^="Run Node"]');
    await runButton.click({ modifiers: ["Shift"] });

    // Shell Command must retain its running visual state
    await expect(shell.locator(".node-running")).toBeVisible();
  });

  test("unsaved execution does not overwrite saved state on reload", async () => {
    // 1-3. create UUID -> Echo with cache disabled, run
    await flowPage.addNode("Generate UUID");
    await flowPage.addNode("Echo");
    await flowPage.connectNodes("UUID", "Echo");
    await flowPage.disableCache("UUID");
    await flowPage.runWorkflowAndExpectComplete();

    const getEchoValue = () =>
      flowPage.getNode("Echo").locator("textarea").first().inputValue();

    const firstUuid = await getEchoValue();
    expect(firstUuid).toBeTruthy();

    // 4. save the workflow
    await flowPage.saveWorkflow("state-persist-test");
    await flowPage.expectSaveComplete();

    // 5. run again (generates a new UUID), do NOT save
    await flowPage.runWorkflowAndExpectComplete();
    // wait for the Echo node to reflect the new UUID
    const echoTextarea = flowPage.getNode("Echo").locator("textarea").first();
    await expect(echoTextarea).not.toHaveValue(firstUuid, { timeout: 5000 });
    const secondUuid = await getEchoValue();
    expect(secondUuid).toBeTruthy();

    // 6. create a new workflow (confirms unsaved changes dialog)
    await flowPage.createNewWorkflow({ ignoreUnsaved: true });

    // 7. open the saved workflow — should have the original UUID
    await flowPage.loadWorkflowAndVerify("state-persist-test", [
      "UUID",
      "Echo",
    ]);

    const restoredUuid = await getEchoValue();
    expect(restoredUuid).toEqual(firstUuid);
  });
});
