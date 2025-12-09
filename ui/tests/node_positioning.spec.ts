import { test } from "@playwright/test";
import { FlowPage } from "./pages/FlowPage";

test.describe("Node Positioning", () => {
  let flowPage: FlowPage;

  test.beforeEach(async ({ page }) => {
    flowPage = new FlowPage(page, { debug: true });
    await flowPage.goto();
    await flowPage.createNewWorkflow({ ignoreUnsaved: true });
  });

  test("add two Echo nodes and connect them", async () => {
    // add first Echo node
    await flowPage.addNode("Echo");
    await flowPage.expectNodeCount(1);

    // add second Echo node (auto-positioned to avoid overlap)
    await flowPage.addNode("Echo");
    await flowPage.expectNodeCount(2);

    // get node IDs and verify no overlap
    const ids = await flowPage.getAllNodeIds();
    await flowPage.expectNodesDoNotOverlap(ids[0], ids[1]);

    // connect the nodes
    await flowPage.connectNodesById(ids[0], ids[1]);
    await flowPage.expectEdgeCount(1);
  });

  test("select node by clicking header", async () => {
    // add two nodes
    await flowPage.addNode("Echo");
    await flowPage.addNode("Echo");
    await flowPage.expectNodeCount(2);

    // select the first Echo node using the high-level method
    await flowPage.selectNode("Echo");

    // verify only one node is selected
    await flowPage.expectSelectedNodeCount(1);
  });
});
