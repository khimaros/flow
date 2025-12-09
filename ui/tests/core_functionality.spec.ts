import { test, expect } from "@playwright/test";
import { FlowPage } from "./pages/FlowPage";

test.describe("Flow UI Core Functionality", () => {
  let flowPage: FlowPage;

  test.beforeEach(async ({ page }) => {
    flowPage = new FlowPage(page, { debug: true });
    await flowPage.goto();
    await flowPage.createNewWorkflow({ ignoreUnsaved: true });
  });

  test("workflow lifecycle: create and save", async () => {
    const timestamp = Date.now();
    const name = `test_wf_${timestamp}`;

    // add a node so we have something to save
    await flowPage.addNode("Echo");
    await flowPage.expectNodeCount(1);

    // save the workflow
    await flowPage.selectSidebarTab("Workflows");
    await flowPage.saveWorkflow(name);
    await expect(flowPage.page.getByText(name).first()).toBeVisible({
      timeout: 5000,
    });
  });

  test("canvas: add and connect nodes", async () => {
    // add two Echo nodes via context menu
    await flowPage.addNode("Echo");
    await flowPage.expectNodeCount(1);

    await flowPage.addNode("Echo");
    await flowPage.expectNodeCount(2);

    // connect nodes
    const ids = await flowPage.getAllNodeIds();
    await flowPage.connectNodesById(ids[0], ids[1]);
    await flowPage.expectEdgeCount(1);
  });

  test("canvas: delete node via context menu", async () => {
    // add a node
    await flowPage.addNode("Echo");
    await flowPage.expectNodeCount(1);

    // delete via context menu
    await flowPage.deleteNode("Echo");
    await flowPage.expectNoNodes();
  });

  test("keyboard shortcuts help overlay", async () => {
    await flowPage.openKeyboardShortcuts();
    await expect(flowPage.page.getByText("Ctrl+Shift+Enter")).toBeVisible();
    await flowPage.closeKeyboardShortcuts();
  });

  test("sidebar tab switching", async () => {
    await flowPage.selectSidebarTab("Workflows");
    await expect(flowPage.page.locator(".sidebar-tab.active", { hasText: "Workflows" })).toBeVisible();
    await flowPage.selectSidebarTab("Nodes");
    await flowPage.selectSidebarTab("Queue");
  });

  test("graph navigation with bracket keys", async () => {
    // create two connected nodes using the composite helper
    const { sourceId, targetId } = await flowPage.createAndConnectNodes(
      "Echo",
      "Echo",
    );

    // use ] to select first node (starts with no selection after createAndConnectNodes)
    await flowPage.navigateNodesWithKeyboard("next", sourceId);

    // ] again selects next node
    await flowPage.navigateNodesWithKeyboard("next", targetId);

    // [ goes back
    await flowPage.navigateNodesWithKeyboard("previous", sourceId);
  });

  test("node selection via header click", async () => {
    await flowPage.addNode("Echo");
    await flowPage.expectNodeCount(1);

    // select the node
    await flowPage.selectNode("Echo");

    // verify selection
    await expect(flowPage.getNode("Echo")).toHaveClass(/selected/);
  });

  test("move node", async () => {
    await flowPage.addNode("Echo");
    await flowPage.expectNodeCount(1);

    // move the node
    await flowPage.moveNode("Echo", 100, 50);

    // verify node is still visible
    await expect(flowPage.getNode("Echo")).toBeVisible();
  });
});
