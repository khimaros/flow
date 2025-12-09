import { test, expect } from "@playwright/test";
import { FlowPage } from "./pages/FlowPage";

test.describe.serial("Manual Scenario", () => {
  let flowPage: FlowPage;

  test.beforeAll(async ({ browser }) => {
    const context = await browser.newContext();
    const page = await context.newPage();
    flowPage = new FlowPage(page, { debug: true });
    await flowPage.goto();
    // start with a clean canvas
    await flowPage.createNewWorkflow({ ignoreUnsaved: true });
  });

  test.afterAll(async () => {
    await flowPage.page.close();
  });

  // ===========================================
  // PHASE 1: Basic node creation and workflow execution
  // ===========================================

  test("add UUID node via context menu", async () => {
    await flowPage.addNode("Generate UUID");
    await expect(flowPage.getNode("UUID")).toBeVisible();
  });

  test("refresh page, verify UUID node persists", async () => {
    await flowPage.refreshPage();
    await flowPage.expectNodeVisible("UUID");
  });

  test("resize and move UUID node", async () => {
    // resize using handle (shrink)
    await flowPage.resizeNode("UUID", -50, -50);

    // get position after resize (viewport may have adjusted)
    const posAfterResize = await flowPage.getNodePosition("UUID");

    // move node left and up
    await flowPage.moveNode("UUID", -50, -50);

    // verify node moved left from its post-resize position
    const finalPos = await flowPage.getNodePosition("UUID");
    expect(finalPos.x).toBeLessThan(posAfterResize.x);
  });

  test("run workflow, verify completed", async () => {
    await flowPage.runWorkflowAndExpectComplete();
  });

  test("run workflow again, verify cached", async () => {
    await flowPage.runWorkflowAndExpectCached();
  });

  test("run workflow (force), verify completed without cache", async () => {
    await flowPage.runWorkflowAndExpectComplete(true);
  });

  // ===========================================
  // PHASE 2: Workflow save/load
  // ===========================================

  test("new workflow shows unsaved changes dialog, cancel keeps nodes", async () => {
    flowPage.page.once("dialog", async (dialog) => {
      expect(dialog.message()).toContain("unsaved changes");
      await dialog.dismiss();
    });
    await flowPage.createNewWorkflow();
    await expect(flowPage.getNode("UUID")).toBeVisible();
  });

  test('save workflow as "uuid-echo"', async () => {
    await flowPage.saveWorkflow("uuid-echo");
    await expect(flowPage.page.getByText("uuid-echo").first()).toBeVisible();
  });

  // ===========================================
  // PHASE 3: Sidebar and queue
  // ===========================================

  test("open sidebar, verify queue shows completed jobs", async () => {
    await flowPage.selectSidebarTab("Queue");
    await expect(flowPage.page.getByText("Completed").first()).toBeVisible();
    const count = await flowPage.page.getByText("Completed").count();
    expect(count).toBeGreaterThanOrEqual(3);
  });

  test("run workflow (force), verify job added to queue", async () => {
    await flowPage.runWorkflowAndExpectComplete(true);
    const count = await flowPage.page.getByText("Completed").count();
    expect(count).toBeGreaterThanOrEqual(4);
  });

  // ===========================================
  // PHASE 4: Add second node and connect
  // ===========================================

  test("add Echo node from sidebar", async () => {
    await flowPage.addNodeFromSidebar("Echo");
    await expect(flowPage.getNode("Echo")).toBeVisible();
  });

  test("move Echo node east", async () => {
    await flowPage.moveNodeByGrid("Echo", "east", 2);
  });

  test("connect UUID to Echo", async () => {
    await flowPage.connectNodes("UUID", "Echo");
    await expect(flowPage.edges.first()).toBeVisible();
  });

  test("run workflow with connected nodes", async () => {
    await flowPage.runWorkflowAndExpectComplete();
  });

  // ===========================================
  // PHASE 5: New workflow, reload saved
  // ===========================================

  test("new workflow shows unsaved dialog, cancel", async () => {
    flowPage.page.once("dialog", async (dialog) => {
      expect(dialog.message()).toContain("unsaved changes");
      await dialog.dismiss();
    });
    await flowPage.createNewWorkflow();
  });

  test("save workflow, then create new empty workflow", async () => {
    await flowPage.saveWorkflow();
    await flowPage.expectSaveComplete();
    await flowPage.createNewWorkflow();
    await expect(flowPage.nodes).toHaveCount(0);
  });

  test('load "uuid-echo" workflow', async () => {
    await flowPage.loadWorkflowAndVerify("uuid-echo", ["UUID", "Echo"]);
    await flowPage.expectEdgeCount(1);
  });

  // ===========================================
  // PHASE 6: Source view toggle
  // ===========================================

  test("view UUID node source code", async () => {
    await flowPage.viewNodeSource("UUID");
    await flowPage.expectNodeSourceViewVisible("UUID");
  });

  test("hide UUID node source code", async () => {
    await flowPage.closeNodeSourceView("UUID");
    await flowPage.expectNodeSourceViewHidden("UUID");
  });

  // ===========================================
  // PHASE 7: Node context menu actions
  // ===========================================

  test("disable cache on UUID node", async () => {
    await flowPage.disableCache("UUID");
  });

  test("bypass Echo node", async () => {
    await flowPage.bypassNode("Echo");
  });

  test("run workflow with bypassed node", async () => {
    await flowPage.runWorkflowAndExpectComplete();
  });

  test("verify bypassed Echo node has reduced opacity", async () => {
    await flowPage.expectNodeBypassed("Echo");
  });

  test("delete UUID node", async () => {
    await flowPage.deleteNode("UUID");
    await expect(flowPage.getNode("UUID")).toHaveCount(0);
  });

  // ===========================================
  // PHASE 8: Shell Command workflow
  // ===========================================

  test('save as "shell-echo"', async () => {
    await flowPage.selectSidebarTab("Workflows");
    await flowPage.saveWorkflow("shell-echo");
    await flowPage.expectSaveComplete();
    await expect(flowPage.page.getByText("shell-echo").first()).toBeVisible();
  });

  test("close sidebar", async () => {
    await flowPage.closeSidebar();
  });

  test("add Shell Command node", async () => {
    await flowPage.addNode("Shell Command");
    await expect(flowPage.getNode("Shell Command")).toBeVisible();
  });

  test("configure Shell Command node", async () => {
    await flowPage.configureShellCommand("cat", "/etc/debian_version");
  });

  test("connect Shell Command stdout to Echo input", async () => {
    await flowPage.connectNodesByHandle(
      "Shell Command",
      "Echo",
      "stdout",
      "message",
    );
  });

  test("run workflow via keyboard shortcut", async () => {
    await flowPage.runWorkflowWithKeyboardAndExpectComplete();
  });

  // ===========================================
  // PHASE 9: Sidebar state persistence
  // ===========================================

  test("open sidebar, refresh, verify still open", async () => {
    await flowPage.openSidebar();
    await flowPage.waitForSidebarStatePersisted(true);
    await flowPage.refreshPage();
    await flowPage.waitForSidebarState(true);
    expect(await flowPage.isSidebarOpen()).toBe(true);
  });

  test("close sidebar, refresh, verify still closed", async () => {
    await flowPage.closeSidebar();
    await flowPage.waitForSidebarStatePersisted(false);
    await flowPage.refreshPage();
    await flowPage.waitForSidebarState(false);
    expect(await flowPage.isSidebarOpen()).toBe(false);
  });
});
