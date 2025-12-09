import { test, expect } from "@playwright/test";
import { FlowPage } from "./pages/FlowPage";

test.describe("Flow UI E2E", () => {
  let flowPage: FlowPage;

  test.beforeEach(async ({ page }) => {
    flowPage = new FlowPage(page, { debug: true });
    await flowPage.goto();
    await flowPage.createNewWorkflow({ ignoreUnsaved: true });
  });

  test("has title", async ({ page }) => {
    await expect(page).toHaveTitle(/flow/);
  });

  test("sidebar loads and switches tabs", async () => {
    await flowPage.selectSidebarTab("Workflows");
    await expect(flowPage.page.locator(".sidebar-tab.active", { hasText: "Workflows" })).toBeVisible();
    await flowPage.selectSidebarTab("Nodes");
    await flowPage.selectSidebarTab("Queue");
  });

  test("load workflow and run it", async () => {
    await flowPage.loadWorkflow("shell-cat");
    await expect(flowPage.nodes.first()).toBeVisible();
    await flowPage.runWorkflow();
    await flowPage.expectWorkflowStates(["queued"]);
  });

  test("run workflow in force mode", async () => {
    await flowPage.loadWorkflow("shell-cat");
    await flowPage.runWorkflow(true);
    await flowPage.expectWorkflowStates(["queued"]);
  });

  test("keyboard shortcuts help", async () => {
    await flowPage.openKeyboardShortcuts();
    await expect(flowPage.page.getByText("Ctrl+Shift+Enter")).toBeVisible();
    await flowPage.closeKeyboardShortcuts();
  });

  test("add and move a node", async () => {
    await flowPage.addNode("Echo");
    await flowPage.expectNodeCount(1);

    // move the node
    await flowPage.moveNode("Echo", 100, 50);

    // verify node is still visible
    await expect(flowPage.getNode("Echo")).toBeVisible();
  });

  test("connect two nodes", async () => {
    await flowPage.addNode("Echo");
    await flowPage.addNode("Echo");
    await flowPage.expectNodeCount(2);

    const ids = await flowPage.getAllNodeIds();
    await flowPage.connectNodesById(ids[0], ids[1]);
    await flowPage.expectEdgeCount(1);
  });
});
