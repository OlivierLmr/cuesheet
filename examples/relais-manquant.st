# drawn: the reliable-broadcast run where n0 crashes mid-broadcast
title "A relay that never happened"
participants n0 n1 n2 n3

  0  n0 asked do_broadcast
  0  n0 deliver
  0  n0 -> n1 .rb +10
 10  n1 deliver
 10  n1 -> n0 .rb +20
 40  n1 -> n2 .rb +20
 60  n2 deliver
 60  n2 -> n0 .rb +20
 69  n1 -> n3 .rb +20
 89  n3 deliver
100  n0 crash
103  n3 -> n1 .rb +20
159  n0 -> n2 .rb +20
174  n2 -> n1 .rb +20
297  n3 -> n2 .rb +20
330  n0 -> n3 .rb +20
357  n2 -> n3 .rb +20
377  run end quiescent
