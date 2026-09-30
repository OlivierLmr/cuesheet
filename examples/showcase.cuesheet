title "Everything the language can say"
participants n0 n1 n2 n3

  0  n0 asked do_broadcast
  0  n0 deliver
  0  n0 -> n1 .rb +10
  0  n0 -> n2 .rb @35
 10  n1 deliver
 20  n1 -x n2 .rb
 30  n1 enter_cs +40
 35  n2 deliver
 50  network split n1 n3 +60
 60  n2 enter_cs
 80  n2 exit_cs
 90  n0 -> n1 .rb .slow "request, 2" +30 {
       {"seq": 4, "round": 2}
     }
100  n1 enter_cs
110  n1 crash
125  n0 -> n0 .retry +20
130  n1 -> n2 .rb +10
140  n2 died
150  n0 -> n3 .rb +40
160  n1 recover
170  run end time-limit
