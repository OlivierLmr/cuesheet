# A run with a partition: n1 and n3 are cut off from n0 and n2, and one node
# crashes inside the window and comes back after it heals.
title "A split, and a node that came back"
participants n0 n1 n2 n3

  0  n0 asked do_broadcast
  0  n0 -> n1 .rb +12
  0  n0 -> n2 .rb +9
 12  n1 deliver
  9  n2 deliver
 20  network split n1 n3 +90
 25  n1 -> n2 .rb +70
 30  n2 -> n0 .ack +8
 40  n3 paused +50
 55  n0 -> n3 .rb +20
 60  n1 crash
 95  n2 -> n1 .rb +10
110  n1 recover
115  n1 -> n0 .ack +14
120  n3 deliver
140  n0 -> n0 .retry +15
150  n2 -x n3 .rb
175  run end quiescent
