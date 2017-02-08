Hoplight aims to be a service that allows applications to implement features that would normally require an always-on server.

It is not done.

Agents in the network build a collection of neighbor agents. 
An agent may send a packet to a neighbor requesting that it does work.
An agent may process such packets from its neighbors.
Agents maintain a balance of trade with their neighbors.
They will typically only elect do requested work if they find the neighbor to be, on net, useful.
"Work" takes the form of a program (see `vm`), and can involve storing data, retrieving data, and sending work requests to other neighbors.
