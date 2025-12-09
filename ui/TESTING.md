- right click canvas, verify context menu with node list appears
- choose UUID node in menu, verify UUID node created
- refresh page, verify UUID node still present on canvas
- resize shrink the UUID node to minimum, move it left and up a bit, verify new position/size
- click "Run Workflow", verify "Running" and "Completed"
- click "Run Workflow" again, verify "Running", and "Cached"
- switch to and click "Run Workflow (force)", verify "Running" and "Completed"

- click "New Workflow", ensure "unsaved changes" dialog opens, click Cancel
- click "Save", name the workflow "uuid-echo" and click OK
- click the button to open the sidebar, verify that it's opened
- click sidebar "Queue" tab, verify three completed jobs the list
- click "Run workflow (force)" and verify that the job is added to queue, in progress, completed

- click sidebar "Nodes" tab
- drag an Echo node to empty canvas area two gridlines to the right of UUID node
- click the button to close the sidebar
- move the Echo node a few grid lines to the east
- drag/drop attach output from the UUID node to input of the Echo node
- click "Run Workflow" again, verify "Running", and "Completed" for both nodes

- click "New Workflow", verify "unsaved changes" shows, then Cancel
- click "Save", click "New Workflow", verify empty canvas
- press the "B" key to open the sidebar, and verify opening
- click sidebar "Workflows" tab, click to load "uuid-echo", ensure two Echo nodes and edge

- click the "View Source" button on UUID node, verify code view
- click the button to return to normal node view
- right click UUID node, Disable Cache
- right click Echo node, and click Bypass Node
- click Run Workflow and verify only UUID node "Running" then "Completed"
- verify Echo node has input updated but wasn't executed
- right click UUID node and Delete, verify deleted
- open sidebar, navigate to "Workflows" tab
- click "Save New Workflow", enter "shell-echo", click OK
- verify "shell-echo" now in Workflows list
- press "B" to close the sidebar and verify closure

- right click canvas, add "Shell Command", verify node created
- edit the command input to `cat` and args to `/etc/debian_version`
- drag from stdout output to the input on the Echo node, verify connection
- press Ctrl+Shift+Enter, verify that both nodes Running -> Completed

- open sidebar, verify sidebar opened
- refresh page, verify sidebar still open
- close sidebar
- refresh page, verify sidebar closed
