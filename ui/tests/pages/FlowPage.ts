import { Page, Locator, expect } from "@playwright/test";

interface BoundingBox {
  x: number;
  y: number;
  width: number;
  height: number;
}

interface Position {
  x: number;
  y: number;
}

export class FlowPage {
  readonly page: Page;
  debug: boolean = false;

  // layout constants
  private static readonly GRID_SIZE = 15;
  private static readonly AUTO_POSITION_COLS = 3;
  private static readonly AUTO_POSITION_X_START = 150;
  private static readonly AUTO_POSITION_Y_START = 150;
  private static readonly AUTO_POSITION_X_SPACING = 350;
  private static readonly AUTO_POSITION_Y_SPACING = 280;

  /**
   * Convert a cardinal direction to x/y offsets.
   */
  private static directionToOffset(
    direction: "north" | "south" | "east" | "west",
    distance: number,
  ): Position {
    switch (direction) {
      case "north":
        return { x: 0, y: -distance };
      case "south":
        return { x: 0, y: distance };
      case "east":
        return { x: distance, y: 0 };
      case "west":
        return { x: -distance, y: 0 };
    }
  }

  constructor(page: Page, options?: { debug?: boolean }) {
    this.page = page;
    this.debug = options?.debug ?? false;
  }

  private log(message: string, ...args: unknown[]) {
    if (this.debug) {
      console.log(`[FlowPage] ${message}`, ...args);
    }
  }

  // ===========================================
  // GETTERS
  // ===========================================

  get canvas(): Locator {
    return this.page.locator(".react-flow__pane");
  }

  get nodes(): Locator {
    return this.page.locator(".react-flow__node");
  }

  get edges(): Locator {
    return this.page.locator(".react-flow__edge");
  }

  get contextMenu(): Locator {
    return this.page.locator(".context-menu");
  }

  getNode(name: string): Locator {
    return this.page
      .locator(".react-flow__node")
      .filter({ hasText: name })
      .first();
  }

  getNodeById(id: string): Locator {
    return this.page.locator(`.react-flow__node[data-id="${id}"]`);
  }

  // ===========================================
  // LOW-LEVEL PRIMITIVES
  // ===========================================

  /**
   * Get the header element and bounding box for a node locator.
   */
  private async getNodeHeaderBoxFromLocator(
    node: Locator,
    identifier: string,
  ): Promise<{ header: Locator; box: BoundingBox }> {
    await expect(node).toBeVisible();

    const header = node.locator('[data-node-header="true"]');
    const box = await header.boundingBox();
    if (!box)
      throw new Error(`Could not get header box for node: ${identifier}`);

    return { header, box };
  }

  /**
   * Get the header element and bounding box for a node by name.
   */
  async getNodeHeaderBox(
    name: string,
  ): Promise<{ header: Locator; box: BoundingBox }> {
    return this.getNodeHeaderBoxFromLocator(this.getNode(name), name);
  }

  /**
   * Get bounding box for a node by ID.
   */
  async getNodeHeaderBoxById(
    id: string,
  ): Promise<{ header: Locator; box: BoundingBox }> {
    return this.getNodeHeaderBoxFromLocator(this.getNodeById(id), id);
  }

  /**
   * Click on a node's header.
   */
  async clickNodeHeader(name: string, button: "left" | "right" = "left") {
    const { box } = await this.getNodeHeaderBox(name);
    // click on header label area (avoid buttons on the right)
    await this.page.mouse.click(box.x + 50, box.y + box.height / 2, { button });
  }

  /**
   * Calculate auto-position for a new node based on current node count.
   */
  async calculateAutoPosition(): Promise<Position> {
    const nodeCount = await this.nodes.count();
    const col = nodeCount % FlowPage.AUTO_POSITION_COLS;
    const row = Math.floor(nodeCount / FlowPage.AUTO_POSITION_COLS);
    return {
      x:
        FlowPage.AUTO_POSITION_X_START + col * FlowPage.AUTO_POSITION_X_SPACING,
      y:
        FlowPage.AUTO_POSITION_Y_START + row * FlowPage.AUTO_POSITION_Y_SPACING,
    };
  }

  /**
   * Get a connection handle from a node.
   */
  getHandle(
    node: Locator,
    position: "left" | "right",
    selector?: string,
  ): Locator {
    if (selector) {
      return node.locator(selector).first();
    }
    return node
      .locator(`.react-flow__handle[data-handlepos="${position}"]`)
      .first();
  }

  /**
   * Connect two node locators via their handles.
   */
  async connectNodeLocators(
    sourceNode: Locator,
    targetNode: Locator,
    sourceHandle?: string,
    targetHandle?: string,
  ) {
    const srcHandle = this.getHandle(sourceNode, "right", sourceHandle);
    const tgtHandle = this.getHandle(targetNode, "left", targetHandle);

    await srcHandle.dragTo(tgtHandle, { force: true });
    await expect(this.edges.first()).toBeVisible();
  }

  /**
   * Wrapper that verifies a node was created after an action.
   */
  async expectNodeCreated(name: string, action: () => Promise<void>) {
    const countBefore = await this.page
      .locator(".react-flow__node")
      .filter({ hasText: name })
      .count();
    await action();
    await expect(
      this.page.locator(".react-flow__node").filter({ hasText: name }),
    ).toHaveCount(countBefore + 1);
  }

  // ===========================================
  // STATE MANAGEMENT
  // ===========================================

  /**
   * Check if sidebar is currently open.
   */
  async isSidebarOpen(): Promise<boolean> {
    return (await this.page.locator(".sidebar-tab").count()) > 0;
  }

  /**
   * Ensure sidebar is open (idempotent).
   */
  async ensureSidebarOpen() {
    if (await this.isSidebarOpen()) return;

    const showBtn = this.page.locator('button[title="Show Sidebar"]');
    if (await showBtn.isVisible()) {
      await showBtn.click();
      await expect(this.page.locator(".sidebar-tab").first()).toBeVisible();
    }
  }

  /**
   * Ensure sidebar is closed (idempotent).
   */
  async ensureSidebarClosed() {
    if (!(await this.isSidebarOpen())) return;

    const hideBtn = this.page.locator('button[title="Hide Sidebar"]');
    if (await hideBtn.isVisible()) {
      await hideBtn.click();
      await expect(this.page.locator(".sidebar-tab")).toHaveCount(0);
    }
  }

  /**
   * Dismiss any open overlays (context menus, modals, etc).
   */
  async dismissOverlays() {
    for (let i = 0; i < 3; i++) {
      await this.page.keyboard.press("Escape");
      await this.page.waitForTimeout(30);
    }
  }

  /**
   * Dismiss all visible toasts.
   */
  async dismissToasts() {
    const toasts = this.page.locator(".toast");
    const toastCount = await toasts.count();
    for (let i = 0; i < toastCount; i++) {
      const closeBtn = toasts
        .nth(i)
        .locator('button, .close, [aria-label="Close"]')
        .first();
      if ((await closeBtn.count()) > 0) {
        await closeBtn.click({ force: true }).catch(() => {});
      }
    }
  }

  /**
   * Click canvas to ensure focus and deselect nodes.
   */
  async focusCanvas() {
    await this.canvas.click({ position: { x: 10, y: 10 }, force: true });
    await this.page.waitForTimeout(50);
  }

  // ===========================================
  // NAVIGATION & SETUP
  // ===========================================

  async goto() {
    await this.page.goto("/");
    await expect(this.page.locator(".react-flow__pane")).toBeVisible();
  }

  /**
   * Prepare canvas for any interaction. Ensures a clean, predictable state.
   */
  async prepareCanvas() {
    this.log("prepareCanvas: ensuring clean state");

    await this.dismissOverlays();
    await this.dismissToasts();
    await this.ensureSidebarClosed();
    await this.resetViewport();
    await this.focusCanvas();

    this.log("prepareCanvas: done");
  }

  /**
   * Reset viewport to a known good state.
   */
  async resetViewport() {
    await this.fitView();
    await this.zoomOut(5);
    this.log("resetViewport: viewport reset complete");
  }

  // ===========================================
  // NODE CREATION - Context Menu
  // ===========================================

  /**
   * Add a node via right-click context menu.
   */
  async addNode(name: string, x?: number, y?: number) {
    this.log(`addNode: adding "${name}"`);

    await this.prepareCanvas();

    // calculate position
    let posX = x;
    let posY = y;
    if (posX === undefined || posY === undefined) {
      const autoPos = await this.calculateAutoPosition();
      posX = posX ?? autoPos.x;
      posY = posY ?? autoPos.y;
    }

    this.log(`addNode: positioning at canvas (${posX}, ${posY})`);

    await this.expectNodeCreated(name, async () => {
      // right-click near top of canvas so context menu has room
      await this.canvas.click({
        button: "right",
        position: { x: posX!, y: 50 },
        force: true,
      });
      await expect(this.page.locator(".context-menu-item").first()).toBeVisible(
        { timeout: 2000 },
      );

      // click the menu item
      const menuItem = this.page
        .locator(".context-menu-item")
        .filter({ hasText: new RegExp(`^${name}$`) });
      await menuItem.scrollIntoViewIfNeeded();
      await menuItem.click();
    });

    this.log(`addNode: "${name}" created successfully`);
  }

  // ===========================================
  // NODE CREATION - Sidebar Drag
  // ===========================================

  /**
   * Add a node by dragging from the sidebar.
   */
  async addNodeFromSidebar(name: string) {
    this.log(`addNodeFromSidebar: adding "${name}"`);

    await this.prepareCanvas();

    await this.ensureSidebarOpen();
    await this.page
      .locator(".sidebar-tab")
      .filter({ hasText: "Nodes" })
      .click();
    await expect(
      this.page.locator(".sidebar-tab.active").filter({ hasText: "Nodes" }),
    ).toBeVisible();

    const nodeItem = this.page
      .locator('[draggable="true"]')
      .filter({ hasText: name })
      .first();
    await expect(nodeItem).toBeVisible();

    const { x, y } = await this.calculateAutoPosition();
    this.log(`addNodeFromSidebar: positioning at (${x}, ${y})`);

    await this.expectNodeCreated(name, async () => {
      await nodeItem.dragTo(this.canvas, {
        targetPosition: { x, y },
        force: true,
      });
    });

    await this.ensureSidebarClosed();

    this.log(`addNodeFromSidebar: "${name}" created successfully`);
  }

  /**
   * Drag a node from the sidebar to a specific position.
   */
  async dragNodeFromSidebar(
    name: string,
    x: number,
    y: number,
    options?: { closeSidebar?: boolean },
  ) {
    this.log(`dragNodeFromSidebar: "${name}" to (${x}, ${y})`);

    await this.dismissOverlays();

    await this.ensureSidebarOpen();
    await this.page
      .locator(".sidebar-tab")
      .filter({ hasText: "Nodes" })
      .click();
    await expect(
      this.page.locator(".sidebar-tab.active").filter({ hasText: "Nodes" }),
    ).toBeVisible();

    const nodeItem = this.page
      .locator('[draggable="true"]')
      .filter({ hasText: name })
      .first();
    await expect(nodeItem).toBeVisible();

    await this.expectNodeCreated(name, async () => {
      await nodeItem.dragTo(this.canvas, {
        targetPosition: { x, y },
        force: true,
      });
    });

    if (options?.closeSidebar) {
      await this.ensureSidebarClosed();
    }

    this.log(`dragNodeFromSidebar: "${name}" created successfully`);
  }

  /**
   * Drag a node from sidebar relative to an existing node.
   */
  async dragNodeRelativeTo(
    name: string,
    relativeTo: string,
    direction: "north" | "south" | "east" | "west",
    gridLines = 2,
  ) {
    const refNode = this.getNode(relativeTo);
    await expect(refNode).toBeVisible();
    const refBox = await refNode.boundingBox();
    if (!refBox)
      throw new Error(`Could not get bounding box for node: ${relativeTo}`);

    // calculate direction-based offset including node size
    const nodeSizeOffset =
      direction === "north" || direction === "south"
        ? refBox.height
        : refBox.width;
    const gridOffset = gridLines * FlowPage.GRID_SIZE;
    const { x: dx, y: dy } = FlowPage.directionToOffset(
      direction,
      nodeSizeOffset + gridOffset,
    );

    let x = refBox.x + refBox.width / 2 + dx;
    let y = refBox.y + refBox.height / 2 + dy;

    // convert to canvas-relative coordinates
    const canvasBox = await this.canvas.boundingBox();
    if (canvasBox) {
      x -= canvasBox.x;
      y -= canvasBox.y;
    }

    await this.dragNodeFromSidebar(name, x, y);
  }

  // ===========================================
  // NODE SELECTION & INTERACTION
  // ===========================================

  /**
   * Select a node by clicking on its header.
   */
  async selectNode(name: string) {
    await this.prepareCanvas();

    await this.clickNodeHeader(name, "left");
    await expect(this.getNode(name)).toHaveClass(/selected/);

    this.log(`selectNode: "${name}" selected`);
  }

  /**
   * Open context menu on a node.
   */
  async openNodeContextMenu(name: string) {
    await this.prepareCanvas();

    await this.clickNodeHeader(name, "right");
    await expect(this.page.locator(".context-menu-item").first()).toBeVisible();

    this.log(`openNodeContextMenu: menu opened for "${name}"`);
  }

  /**
   * Delete a node using keyboard (select + Delete key).
   */
  async deleteNodeWithKeyboard(name: string) {
    await this.selectNode(name);
    await this.page.keyboard.press("Delete");
    await expect(this.getNode(name)).toHaveCount(0);
    this.log(`deleteNodeWithKeyboard: "${name}" deleted`);
  }

  /**
   * Delete a node via context menu.
   */
  async deleteNodeViaContextMenu(name: string) {
    await this.openNodeContextMenu(name);
    await this.page
      .locator(".context-menu-item")
      .filter({ hasText: "Delete" })
      .click();
    await expect(this.getNode(name)).toHaveCount(0);
    this.log(`deleteNodeViaContextMenu: "${name}" deleted`);
  }

  /**
   * Delete a node (alias for deleteNodeWithKeyboard for backwards compatibility).
   */
  async deleteNode(name: string) {
    await this.deleteNodeWithKeyboard(name);
  }

  async bypassNode(name: string) {
    await this.openNodeContextMenu(name);
    await this.page
      .locator(".context-menu-item")
      .filter({ hasText: "Bypass Node" })
      .click();
  }

  async disableCache(name: string) {
    await this.openNodeContextMenu(name);
    await this.page
      .locator(".context-menu-item")
      .filter({ hasText: "Skip Cache" })
      .click();
  }

  async runNode(name: string) {
    await this.prepareCanvas();

    const node = this.getNode(name);
    const runButton = node.locator('[title^="Run Node"]');
    await runButton.click();
  }

  async fillNodeInput(nodeName: string, value: string, index = 0) {
    await this.prepareCanvas();

    const node = this.getNode(nodeName);
    const input = node.locator('textarea, input[type="text"]').nth(index);
    await input.fill(value);
  }

  async fillNodeInputById(nodeId: string, value: string, index = 0) {
    await this.prepareCanvas();

    const node = this.getNodeById(nodeId);
    const input = node.locator('textarea, input[type="text"]').nth(index);
    await input.fill(value);
  }

  // ===========================================
  // NODE CONNECTIONS
  // ===========================================

  /**
   * Connect two nodes by name.
   */
  async connectNodes(
    sourceName: string,
    targetName: string,
    sourceHandle?: string,
    targetHandle?: string,
  ) {
    await this.prepareCanvas();

    const sourceNode = this.getNode(sourceName);
    const targetNode = this.getNode(targetName);

    await expect(sourceNode).toBeVisible();
    await expect(targetNode).toBeVisible();

    await this.connectNodeLocators(
      sourceNode,
      targetNode,
      sourceHandle,
      targetHandle,
    );

    this.log(`connectNodes: ${sourceName} -> ${targetName} connected`);
  }

  /**
   * Connect two nodes by ID.
   */
  async connectNodesById(
    sourceId: string,
    targetId: string,
    sourceHandle?: string,
    targetHandle?: string,
  ) {
    await this.prepareCanvas();

    const sourceNode = this.getNodeById(sourceId);
    const targetNode = this.getNodeById(targetId);

    await this.connectNodeLocators(
      sourceNode,
      targetNode,
      sourceHandle,
      targetHandle,
    );
  }

  /**
   * Connect nodes by their position index in the node list.
   */
  async connectNodesByPosition(sourceIdx: number, targetIdx: number) {
    const nodes = await this.nodes.all();
    await this.connectNodeLocators(nodes[sourceIdx], nodes[targetIdx]);
  }

  /**
   * Create two connected nodes and return their IDs.
   */
  async createAndConnectNodes(
    sourceName: string,
    targetName: string,
  ): Promise<{ sourceId: string; targetId: string }> {
    await this.addNode(sourceName, 300, 200);
    const sourceNode = this.getNode(sourceName);
    const sourceId = await sourceNode.getAttribute("data-id");

    await this.dragNodeFromSidebar(targetName, 700, 200, {
      closeSidebar: true,
    });
    const allNodes = await this.nodes.all();
    const targetNode = allNodes[allNodes.length - 1];
    const targetId = await targetNode.getAttribute("data-id");

    await this.connectNodesByPosition(0, 1);

    if (!sourceId || !targetId) throw new Error("Could not get node IDs");
    return { sourceId, targetId };
  }

  // ===========================================
  // NODE MOVEMENT
  // ===========================================

  async moveNode(name: string, dx: number, dy: number) {
    await this.prepareCanvas();

    const node = this.getNode(name);
    await expect(node).toBeVisible();
    const box = await node.boundingBox();
    if (!box) throw new Error(`Could not get bounding box for node: ${name}`);

    const startX = box.x + box.width / 2;
    const startY = box.y + 15;
    await this.page.mouse.move(startX, startY);
    await this.page.mouse.down();
    await this.page.mouse.move(startX + dx, startY + dy, { steps: 5 });
    await this.page.mouse.up();
  }

  async moveNodeByGrid(
    name: string,
    direction: "north" | "south" | "east" | "west",
    gridLines = 2,
  ) {
    const distance = gridLines * FlowPage.GRID_SIZE;
    const { x: dx, y: dy } = FlowPage.directionToOffset(direction, distance);
    await this.moveNode(name, dx, dy);
  }

  // ===========================================
  // NODE RESIZING
  // ===========================================

  /**
   * Get current bounds (position and size) of a node.
   */
  async getNodeBounds(name: string): Promise<BoundingBox> {
    const node = this.getNode(name);
    await expect(node).toBeVisible();
    const box = await node.boundingBox();
    if (!box) throw new Error(`Could not get bounding box for node: ${name}`);
    return box;
  }

  /**
   * Get current position of a node.
   */
  async getNodePosition(name: string): Promise<Position> {
    const box = await this.getNodeBounds(name);
    return { x: box.x, y: box.y };
  }

  /**
   * Get current size of a node.
   */
  async getNodeSize(name: string): Promise<{ width: number; height: number }> {
    const box = await this.getNodeBounds(name);
    return { width: box.width, height: box.height };
  }

  /**
   * Resize a node by dragging its resize handle.
   */
  async resizeNode(name: string, deltaWidth: number, deltaHeight: number) {
    // select node first (required for resize handles to be active)
    // note: selectNode calls prepareCanvas internally
    await this.selectNode(name);

    // get fresh bounding box after selection/prepareCanvas
    const node = this.getNode(name);
    const box = await node.boundingBox();
    if (!box) throw new Error(`Could not get bounding box for node: ${name}`);

    // find resize handle - prioritize the specific ReactFlow resize control
    const resizeHandle = node
      .locator(
        ".react-flow__resize-control.bottom.right, .react-resizable-handle, .resize-handle",
      )
      .first();

    if ((await resizeHandle.count()) > 0) {
      // use resize handle if present
      await resizeHandle.hover();
      await this.page.mouse.down();
      await this.page.mouse.move(
        box.x + box.width + deltaWidth,
        box.y + box.height + deltaHeight,
        { steps: 5 },
      );
      await this.page.mouse.up();
    } else {
      // fallback: drag from bottom-right corner
      const cornerX = box.x + box.width - 5;
      const cornerY = box.y + box.height - 5;
      await this.page.mouse.move(cornerX, cornerY);
      await this.page.mouse.down();
      await this.page.mouse.move(cornerX + deltaWidth, cornerY + deltaHeight, {
        steps: 5,
      });
      await this.page.mouse.up();
    }

    this.log(
      `resizeNode: "${name}" resized by (${deltaWidth}, ${deltaHeight})`,
    );
  }

  /**
   * Resize a node to minimum size.
   */
  async resizeNodeToMinimum(name: string) {
    // shrink significantly - the node should stop at its minimum
    await this.resizeNode(name, -500, -500);
    this.log(`resizeNodeToMinimum: "${name}" shrunk to minimum`);
  }

  /**
   * Verify node has expected size (with tolerance).
   */
  async expectNodeSize(
    name: string,
    expectedWidth: number,
    expectedHeight: number,
    tolerance = 10,
  ) {
    const { width, height } = await this.getNodeSize(name);
    expect(width).toBeGreaterThanOrEqual(expectedWidth - tolerance);
    expect(width).toBeLessThanOrEqual(expectedWidth + tolerance);
    expect(height).toBeGreaterThanOrEqual(expectedHeight - tolerance);
    expect(height).toBeLessThanOrEqual(expectedHeight + tolerance);
  }

  /**
   * Verify node position (with tolerance).
   */
  async expectNodePosition(
    name: string,
    expectedX: number,
    expectedY: number,
    tolerance = 10,
  ) {
    const { x, y } = await this.getNodePosition(name);
    expect(x).toBeGreaterThanOrEqual(expectedX - tolerance);
    expect(x).toBeLessThanOrEqual(expectedX + tolerance);
    expect(y).toBeGreaterThanOrEqual(expectedY - tolerance);
    expect(y).toBeLessThanOrEqual(expectedY + tolerance);
  }

  // ===========================================
  // NODE SOURCE VIEW
  // ===========================================

  /**
   * Open source view for a node.
   */
  async viewNodeSource(name: string) {
    await this.prepareCanvas();

    const node = this.getNode(name);
    const viewSourceBtn = node
      .locator(
        'div[title="View Source"], [title="View Source"], button:has-text("View Source")',
      )
      .first();
    await expect(viewSourceBtn).toBeVisible();
    await viewSourceBtn.dispatchEvent("click");
    await this.page.waitForTimeout(100);

    // verify source view appeared
    await expect(node.locator("pre")).toBeVisible();

    this.log(`viewNodeSource: opened source view for "${name}"`);
  }

  /**
   * Close source view and return to normal node view.
   */
  async closeNodeSourceView(name: string) {
    const node = this.getNode(name);
    const closeBtn = node
      .locator(
        'div[title="Hide Source"], [title="Hide Source"], [title="Close Source"], [title="Back"]',
      )
      .first();
    await expect(closeBtn).toBeVisible();
    await closeBtn.dispatchEvent("click");
    await this.page.waitForTimeout(100);

    // verify source view hidden
    await expect(node.locator("pre")).not.toBeVisible();

    this.log(`closeNodeSourceView: closed source view for "${name}"`);
  }

  /**
   * Verify source view is visible for a node.
   */
  async expectNodeSourceViewVisible(name: string) {
    const node = this.getNode(name);
    await expect(node.locator("pre")).toBeVisible();
  }

  /**
   * Verify source view is hidden for a node.
   */
  async expectNodeSourceViewHidden(name: string) {
    const node = this.getNode(name);
    await expect(node.locator("pre")).not.toBeVisible();
  }

  // ===========================================
  // SIDEBAR
  // ===========================================

  async openSidebar() {
    await this.ensureSidebarOpen();
  }

  async closeSidebar() {
    await this.ensureSidebarClosed();
  }

  /**
   * Wait for sidebar state to be persisted to localStorage.
   */
  async waitForSidebarStatePersisted(expectedOpen: boolean) {
    await this.page.waitForFunction(
      (expected) => {
        const saved = localStorage.getItem("flow_autosave");
        if (!saved) return false;
        try {
          const parsed = JSON.parse(saved);
          return parsed.sidebarVisible === expected;
        } catch {
          return false;
        }
      },
      expectedOpen,
      { timeout: 5000 },
    );
  }

  /**
   * Toggle sidebar using keyboard shortcut ('B' key).
   */
  async toggleSidebarWithKeyboard() {
    const wasOpen = await this.isSidebarOpen();
    await this.page.keyboard.press("b");
    await this.page.waitForTimeout(100);

    if (wasOpen) {
      await expect(this.page.locator(".sidebar-tab")).toHaveCount(0);
    } else {
      await expect(this.page.locator(".sidebar-tab").first()).toBeVisible();
    }

    this.log(
      `toggleSidebarWithKeyboard: sidebar ${wasOpen ? "closed" : "opened"}`,
    );
  }

  /**
   * Open sidebar using keyboard shortcut.
   */
  async openSidebarWithKeyboard() {
    if (await this.isSidebarOpen()) return;
    await this.page.keyboard.press("b");
    await expect(this.page.locator(".sidebar-tab").first()).toBeVisible();
  }

  /**
   * Close sidebar using keyboard shortcut.
   */
  async closeSidebarWithKeyboard() {
    if (!(await this.isSidebarOpen())) return;
    await this.page.keyboard.press("b");
    await expect(this.page.locator(".sidebar-tab")).toHaveCount(0);
  }

  async selectSidebarTab(tabName: "Workflows" | "Nodes" | "Queue") {
    await this.ensureSidebarOpen();
    await this.page
      .locator(".sidebar-tab")
      .filter({ hasText: tabName })
      .click();
    await expect(
      this.page.locator(".sidebar-tab.active").filter({ hasText: tabName }),
    ).toBeVisible();
  }

  // ===========================================
  // QUEUE TAB
  // ===========================================

  /**
   * Verify the number of jobs in the queue tab.
   */
  async expectQueueJobCount(count: number) {
    await this.selectSidebarTab("Queue");
    const jobs = this.page.locator(
      '.queue-item, .job-item, [class*="queue"] [class*="item"]',
    );
    await expect(jobs).toHaveCount(count);
  }

  /**
   * Verify a job with specific status exists in the queue.
   */
  async expectQueueHasJob(
    status: "completed" | "running" | "queued" | "failed",
  ) {
    await this.selectSidebarTab("Queue");
    const statusClass = `.job-${status}, .queue-${status}, [class*="${status}"]`;
    await expect(this.page.locator(statusClass).first()).toBeVisible();
  }

  /**
   * Get count of jobs with specific status.
   */
  async getQueueJobCountByStatus(
    status: "completed" | "running" | "queued" | "failed",
  ): Promise<number> {
    await this.selectSidebarTab("Queue");
    const statusClass = `.job-${status}, .queue-${status}, [class*="${status}"]`;
    return await this.page.locator(statusClass).count();
  }

  // ===========================================
  // WORKFLOWS
  // ===========================================

  async createNewWorkflow(options?: { ignoreUnsaved?: boolean }) {
    const btn = this.page.getByRole("button", {
      name: "New Workflow",
      exact: true,
    });

    if (options?.ignoreUnsaved) {
      const dialogHandler = async (
        dialog: import("@playwright/test").Dialog,
      ) => {
        if (dialog.message().includes("unsaved changes")) {
          await dialog.accept();
        }
      };
      this.page.on("dialog", dialogHandler);
      await btn.click();
      await this.page.waitForTimeout(100);
      this.page.off("dialog", dialogHandler);
    } else {
      await btn.click();
    }
  }

  async saveWorkflow(name?: string) {
    if (name) {
      // save As with a new name - use dialog to enter the name
      this.page.once("dialog", async (dialog) => {
        await dialog.accept(name);
      });
      await this.page.getByRole("button", { name: "Save As" }).click();
    } else {
      // save current workflow (may prompt for name if first save)
      await this.page
        .getByRole("button", { name: "Save", exact: true })
        .click();
    }
  }

  async expectSaveComplete() {
    await expect(
      this.page.locator(".toast").filter({ hasText: "Workflow saved" }).first(),
    ).toBeVisible();
  }

  async loadWorkflow(name: string) {
    await this.selectSidebarTab("Workflows");
    await this.page.getByText(name).first().click();
  }

  /**
   * Set run mode (normal or force).
   */
  async setRunMode(force: boolean) {
    const runBtn = this.page
      .getByRole("button")
      .filter({ hasText: /Run Workflow/ })
      .first();
    const text = await runBtn.textContent();
    const isForceMode = text?.includes("force");

    if (force !== isForceMode) {
      // click dropdown to switch mode
      await runBtn.locator("xpath=following-sibling::button").first().click();
      const targetText = force ? "Run Workflow (force)" : "Run Workflow";
      await this.page.getByText(targetText, { exact: !force }).click();
    }
  }

  async runWorkflow(force = false) {
    await this.setRunMode(force);
    const targetName = force ? "Run Workflow (force)" : "Run Workflow";
    await this.page
      .getByRole("button", { name: targetName, exact: true })
      .click();
  }

  /**
   * Run workflow using keyboard shortcut (Ctrl+Shift+Enter).
   */
  async runWorkflowWithKeyboard() {
    await this.page.keyboard.press("Control+Shift+Enter");
    this.log("runWorkflowWithKeyboard: triggered");
  }

  /**
   * Run workflow via keyboard and wait for completion (queued + completed).
   */
  async runWorkflowWithKeyboardAndExpectComplete() {
    await this.runWorkflowWithKeyboard();
    await this.expectWorkflowStates(["queued", "completed"]);
  }

  async expectWorkflowStates(
    states: ("queued" | "running" | "completed" | "cached")[],
  ) {
    for (const state of states) {
      switch (state) {
        case "queued":
          await expect(
            this.page
              .locator(".toast")
              .filter({ hasText: "Job Queued" })
              .first(),
          ).toBeVisible();
          break;
        case "running":
          await expect(
            this.page.locator(".react-flow__node .node-running").first(),
          ).toBeVisible();
          break;
        case "completed":
          // queue is serial server-side; under parallel test workers a
          // long-running job (e.g. shell sleep) can delay this job's start
          await expect(
            this.page
              .locator(".toast")
              .filter({ hasText: "Job Completed" })
              .first(),
          ).toBeVisible({ timeout: 30000 });
          break;
        case "cached":
          await expect(
            this.page.locator('.react-flow__node [data-cached="true"]').first(),
          ).toBeVisible({ timeout: 15000 });
          break;
      }
    }
  }

  async renameWorkflow(oldName: string, newName: string) {
    await this.selectSidebarTab("Workflows");
    const workflowItem = this.page.getByText(oldName).first();
    await workflowItem.click({ button: "right" });
    await expect(this.page.locator(".context-menu-item").first()).toBeVisible();

    this.page.once("dialog", async (dialog) => {
      await dialog.accept(newName);
    });

    await this.page
      .locator(".context-menu-item")
      .filter({ hasText: "Rename" })
      .click();
    await expect(this.page.getByText(newName).first()).toBeVisible();
  }

  async deleteWorkflow(name: string) {
    await this.selectSidebarTab("Workflows");
    const workflowItem = this.page.getByText(name).first();
    await workflowItem.click({ button: "right" });
    await expect(this.page.locator(".context-menu-item").first()).toBeVisible();

    this.page.once("dialog", async (dialog) => {
      await dialog.accept();
    });

    await this.page
      .locator(".context-menu-item")
      .filter({ hasText: "Delete" })
      .click();
    await expect(this.page.getByText(name)).toHaveCount(0);
  }

  // ===========================================
  // PAGE REFRESH & PERSISTENCE
  // ===========================================

  /**
   * Refresh the page and wait for canvas to be ready.
   */
  async refreshPage() {
    await this.page.reload();
    await expect(this.page.locator(".react-flow__pane")).toBeVisible();
    this.log("refreshPage: page reloaded");
  }

  /**
   * Wait for sidebar to reach expected visibility state.
   * Useful after page load when state restore is async.
   */
  async waitForSidebarState(expectedOpen: boolean) {
    if (expectedOpen) {
      await expect(this.page.locator(".sidebar-tab").first()).toBeVisible({
        timeout: 5000,
      });
    } else {
      await expect(this.page.locator(".sidebar-tab")).toHaveCount(0, {
        timeout: 5000,
      });
    }
  }

  /**
   * Verify nodes persist after page refresh.
   */
  async expectNodesPersisted(names: string[]) {
    await this.refreshPage();
    for (const name of names) {
      await expect(this.getNode(name)).toBeVisible();
    }
    this.log(`expectNodesPersisted: verified ${names.length} nodes`);
  }

  /**
   * Verify sidebar state persists after refresh.
   */
  async expectSidebarStatePersisted(expectedOpen: boolean) {
    await this.refreshPage();
    const isOpen = await this.isSidebarOpen();
    expect(isOpen).toBe(expectedOpen);
    this.log(
      `expectSidebarStatePersisted: sidebar ${isOpen ? "open" : "closed"} as expected`,
    );
  }

  // ===========================================
  // CANVAS CONTROLS
  // ===========================================

  async fitView() {
    const fitBtn = this.page.locator('button[title="fit view"]');
    if (await fitBtn.isVisible()) {
      await fitBtn.click();
      await this.page.waitForTimeout(100);
    }
  }

  /**
   * Click a button repeatedly while it remains enabled.
   */
  private async clickButtonRepeatedly(
    selector: string,
    times: number,
    delayMs = 30,
  ) {
    const btn = this.page.locator(selector);
    for (let i = 0; i < times; i++) {
      if (await btn.isEnabled()) {
        await btn.click();
        await this.page.waitForTimeout(delayMs);
      } else {
        break;
      }
    }
  }

  async zoomOut(times = 1) {
    await this.clickButtonRepeatedly('button[title="zoom out"]', times);
  }

  async zoomIn(times = 1) {
    await this.clickButtonRepeatedly('button[title="zoom in"]', times);
  }

  async clickCanvas(x: number, y: number) {
    await this.canvas.click({ position: { x, y }, force: true });
  }

  // ===========================================
  // KEYBOARD SHORTCUTS
  // ===========================================

  async openKeyboardShortcuts() {
    await this.page.keyboard.press("?");
    await expect(this.page.getByText("Keyboard Shortcuts")).toBeVisible();
  }

  async closeKeyboardShortcuts() {
    await this.page.keyboard.press("Escape");
    await expect(this.page.getByText("Keyboard Shortcuts")).not.toBeVisible();
  }

  // ===========================================
  // ASSERTIONS
  // ===========================================

  async expectNodeCount(count: number) {
    await expect(this.nodes).toHaveCount(count);
  }

  async expectEdgeCount(count: number) {
    await expect(this.edges).toHaveCount(count);
  }

  async expectNodeFinished(name: string) {
    const node = this.getNode(name);
    await expect(node.locator(".node-just-finished")).toBeVisible({
      timeout: 15000,
    });
  }

  async expectNoNodes() {
    await expect(this.nodes).toHaveCount(0);
  }

  async expectNodeNotRunning(name: string) {
    const node = this.getNode(name);
    await expect(node.locator(".node-running")).toHaveCount(0);
  }

  async expectNodeCachedColor(name: string) {
    const node = this.getNode(name);
    const frame = node.locator('[data-cached="true"]');
    await expect(frame).toBeVisible({ timeout: 15000 });
    // verify amber glow color was applied (persists beyond animation)
    const color = await frame.evaluate(
      (el) => getComputedStyle(el).getPropertyValue("--finish-glow-color"),
    );
    expect(color.trim()).toBe("#f59e0b");
  }

  async expectNodeVisible(name: string) {
    await expect(this.getNode(name)).toBeVisible();
  }

  async expectNodeNotVisible(name: string) {
    await expect(this.getNode(name)).not.toBeVisible();
  }

  // ===========================================
  // COMPOSITE HELPERS
  // ===========================================

  /**
   * Add a node, fill its first input, and run it.
   */
  async addAndRunNode(name: string, inputValue: string) {
    await this.addNode(name);
    await this.fillNodeInput(name, inputValue, 0);
    await this.runNode(name);
  }

  /**
   * Fill multiple inputs on a node at once.
   */
  async fillNodeInputs(nodeName: string, values: string[]) {
    await this.prepareCanvas();

    const node = this.getNode(nodeName);
    const inputs = node.locator('textarea, input[type="text"]');

    for (let i = 0; i < values.length; i++) {
      await inputs.nth(i).fill(values[i]);
    }

    this.log(
      `fillNodeInputs: "${nodeName}" filled with ${values.length} values`,
    );
  }

  /**
   * Run workflow and wait for completion (queued + completed states).
   * Note: 'running' state is transient and may complete too fast to observe.
   */
  async runWorkflowAndExpectComplete(force = false) {
    await this.runWorkflow(force);
    await this.expectWorkflowStates(["queued", "completed"]);
  }

  /**
   * Run workflow and expect cached result (queued + cached + completed, skips running).
   */
  async runWorkflowAndExpectCached() {
    await this.runWorkflow();
    // check cached and completed concurrently — the cached glow is transient
    // (1500ms) and may expire if we check states sequentially
    await Promise.all([
      expect(
        this.page.locator('.react-flow__node [data-cached="true"]').first(),
      ).toBeVisible({ timeout: 15000 }),
      expect(
        this.page.locator(".toast").filter({ hasText: "Job Completed" }).first(),
      ).toBeVisible({ timeout: 15000 }),
    ]);
  }

  /**
   * Add two nodes and connect them (simplified helper).
   * Returns the node IDs.
   */
  async addNodesAndConnect(
    sourceName: string,
    targetName: string,
  ): Promise<{ sourceId: string; targetId: string }> {
    return await this.createAndConnectNodes(sourceName, targetName);
  }

  /**
   * Setup handler for unsaved changes dialog.
   */
  setupUnsavedChangesHandler(action: "accept" | "dismiss") {
    const handler = async (dialog: import("@playwright/test").Dialog) => {
      if (!dialog.message().includes("unsaved changes")) return;
      if (action === "accept") {
        await dialog.accept();
      } else {
        await dialog.dismiss();
      }
    };
    this.page.on("dialog", handler);
    return () => this.page.off("dialog", handler);
  }

  async getAllNodeIds(): Promise<string[]> {
    const nodes = await this.nodes.all();
    const ids = await Promise.all(
      nodes.map((node) => node.getAttribute("data-id")),
    );
    return ids.filter((id): id is string => id !== null);
  }

  // ===========================================
  // ADVANCED ASSERTIONS
  // ===========================================

  /**
   * Verify a node is bypassed (has reduced opacity).
   */
  async expectNodeBypassed(name: string) {
    const node = this.getNode(name);
    const nodeFrame = node.locator(".node-frame").first();
    await expect(nodeFrame).toHaveCSS("opacity", "0.6");
    this.log(`expectNodeBypassed: "${name}" is bypassed`);
  }

  /**
   * Verify a node is not bypassed (has full opacity).
   */
  async expectNodeNotBypassed(name: string) {
    const node = this.getNode(name);
    const nodeFrame = node.locator(".node-frame").first();
    await expect(nodeFrame).not.toHaveCSS("opacity", "0.6");
  }

  /**
   * Verify two nodes do not overlap.
   */
  async expectNodesDoNotOverlap(id1: string, id2: string) {
    const node1 = this.getNodeById(id1);
    const node2 = this.getNodeById(id2);

    const box1 = await node1.boundingBox();
    const box2 = await node2.boundingBox();

    if (!box1 || !box2) {
      throw new Error("Could not get bounding boxes for nodes");
    }

    const overlapX =
      box1.x < box2.x + box2.width && box1.x + box1.width > box2.x;
    const overlapY =
      box1.y < box2.y + box2.height && box1.y + box1.height > box2.y;
    const overlaps = overlapX && overlapY;

    expect(overlaps).toBe(false);
    this.log(`expectNodesDoNotOverlap: nodes ${id1} and ${id2} do not overlap`);
  }

  /**
   * Verify the number of selected nodes.
   */
  async expectSelectedNodeCount(count: number) {
    const selectedNodes = this.page.locator(".react-flow__node.selected");
    await expect(selectedNodes).toHaveCount(count);
  }

  /**
   * Get the currently selected node (assumes one is selected).
   */
  getSelectedNode(): Locator {
    return this.page.locator(".react-flow__node.selected").first();
  }

  // ===========================================
  // KEYBOARD NAVIGATION
  // ===========================================

  /**
   * Select next node using ] key (keyboard navigation).
   */
  async selectNextNodeWithKeyboard() {
    await this.page.keyboard.press("]");
    await this.page.waitForTimeout(50);
  }

  /**
   * Select previous node using [ key (keyboard navigation).
   */
  async selectPreviousNodeWithKeyboard() {
    await this.page.keyboard.press("[");
    await this.page.waitForTimeout(50);
  }

  /**
   * Navigate through nodes using keyboard and verify selection changes.
   */
  async navigateNodesWithKeyboard(
    direction: "next" | "previous",
    expectedId?: string,
  ) {
    const navigationMethod =
      direction === "next"
        ? this.selectNextNodeWithKeyboard
        : this.selectPreviousNodeWithKeyboard;
    await navigationMethod.call(this);

    if (expectedId) {
      await expect(this.getNodeById(expectedId)).toHaveClass(/selected/);
    }
  }

  // ===========================================
  // WORKFLOW SCENARIOS
  // ===========================================

  /**
   * Complete scenario: create workflow with node, save it, and verify.
   */
  async createAndSaveWorkflow(workflowName: string, nodeName: string) {
    await this.addNode(nodeName);
    await this.expectNodeCount(1);
    await this.selectSidebarTab("Workflows");
    await this.saveWorkflow(workflowName);
    await expect(this.page.getByText(workflowName).first()).toBeVisible({
      timeout: 5000,
    });
    this.log(
      `createAndSaveWorkflow: "${workflowName}" saved with ${nodeName} node`,
    );
  }

  /**
   * Load a workflow and verify it has expected nodes.
   */
  async loadWorkflowAndVerify(workflowName: string, expectedNodes: string[]) {
    await this.loadWorkflow(workflowName);
    for (const nodeName of expectedNodes) {
      await expect(this.getNode(nodeName)).toBeVisible();
    }
    this.log(
      `loadWorkflowAndVerify: "${workflowName}" loaded with ${expectedNodes.length} nodes`,
    );
  }

  /**
   * Execute full run cycle: run workflow and verify completion.
   * Alias for runWorkflowAndExpectComplete with logging.
   */
  async executeAndVerify(force = false) {
    await this.runWorkflowAndExpectComplete(force);
    this.log(`executeAndVerify: workflow executed ${force ? "(force)" : ""}`);
  }

  /**
   * Configure a Shell Command node with command and args.
   */
  async configureShellCommand(command: string, args: string) {
    await this.fillNodeInputs("Shell Command", [command, args]);
  }

  /**
   * Connect nodes using specific handle IDs.
   */
  async connectNodesByHandle(
    sourceName: string,
    targetName: string,
    sourceHandleId: string,
    targetHandleId: string,
  ) {
    const sourceNode = this.getNode(sourceName);
    const targetNode = this.getNode(targetName);

    const sourceId = await sourceNode.getAttribute("data-id");
    const targetId = await targetNode.getAttribute("data-id");

    if (!sourceId || !targetId) {
      throw new Error("Could not get node IDs");
    }

    await this.connectNodesById(
      sourceId,
      targetId,
      `[data-handleid="${sourceHandleId}"]`,
      `[data-handleid="${targetHandleId}"]`,
    );

    this.log(
      `connectNodesByHandle: ${sourceName}:${sourceHandleId} -> ${targetName}:${targetHandleId}`,
    );
  }
}
