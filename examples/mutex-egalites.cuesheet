# converted from diagrammes/mutex-egalites.mmd — Lamport mutex, two rounds
title "Lamport mutex: four processes, two rounds"
participants n0 n1 n2 n3

1000 n0 asked request_cs
1000 n3 asked request_cs
1000 n1 asked request_cs
1000 n2 asked request_cs
1000 n2 -> n1 .mx_request @1010
1000 n0 -> n1 .mx_request @1010
1000 n2 -> n0 .mx_request @1011
1000 n0 -> n3 .mx_request @1011
1000 n3 -> n0 .mx_request @1012
1000 n1 -> n0 .mx_request @1012
1000 n2 -> n3 .mx_request @1012
1000 n3 -> n2 .mx_request @1014
1000 n1 -> n3 .mx_request @1014
1000 n1 -> n2 .mx_request @1015
1000 n0 -> n2 .mx_request @1015
1000 n3 -> n1 .mx_request @1018
1010 n1 -> n2 .mx_reply @1020
1010 n1 -> n0 .mx_reply @1023
1011 n0 -> n2 .mx_reply @1024
1011 n3 -> n0 .mx_reply @1027
1012 n0 -> n3 .mx_reply @1022
1012 n3 -> n2 .mx_reply @1029
1012 n0 -> n1 .mx_reply @1031
1014 n2 -> n3 .mx_reply @1025
1014 n3 -> n1 .mx_reply @1027
1015 n2 -> n1 .mx_reply @1025
1015 n2 -> n0 .mx_reply @1027
1018 n1 -> n3 .mx_reply @1028
1027 n0 enter_cs
1077 n0 exit_cs
1077 n0 -> n3 .mx_release @1087
1077 n0 -> n2 .mx_release @1089
1077 n0 -> n1 .mx_release @1092
1092 n1 enter_cs
1142 n1 exit_cs
1142 n1 -> n2 .mx_release @1152
1142 n1 -> n0 .mx_release @1155
1142 n1 -> n3 .mx_release @1161
1152 n2 enter_cs
1202 n2 exit_cs
1202 n2 -> n0 .mx_release @1212
1202 n2 -> n3 .mx_release @1213
1202 n2 -> n1 .mx_release @1214
1213 n3 enter_cs
1263 n3 exit_cs
1263 n3 -> n1 .mx_release @1273
1263 n3 -> n2 .mx_release @1273
1263 n3 -> n0 .mx_release @1274
4000 n1 asked request_cs
4000 n3 asked request_cs
4000 n2 asked request_cs
4000 n0 asked request_cs
4000 n2 -> n0 .mx_request @4010
4000 n3 -> n1 .mx_request @4011
4000 n0 -> n2 .mx_request @4011
4000 n0 -> n1 .mx_request @4012
4000 n1 -> n2 .mx_request @4013
4000 n2 -> n3 .mx_request @4014
4000 n2 -> n1 .mx_request @4014
4000 n0 -> n3 .mx_request @4015
4000 n3 -> n2 .mx_request @4017
4000 n1 -> n0 .mx_request @4018
4000 n1 -> n3 .mx_request @4019
4000 n3 -> n0 .mx_request @4019
4010 n0 -> n2 .mx_reply @4026
4011 n1 -> n3 .mx_reply @4023
4011 n2 -> n0 .mx_reply @4025
4012 n1 -> n0 .mx_reply @4027
4013 n2 -> n1 .mx_reply @4024
4014 n1 -> n2 .mx_reply @4028
4014 n3 -> n2 .mx_reply @4029
4015 n3 -> n0 .mx_reply @4025
4017 n2 -> n3 .mx_reply @4030
4018 n0 -> n1 .mx_reply @4035
4019 n3 enter_cs
4019 n3 -> n1 .mx_reply @4029
4019 n0 -> n3 .mx_reply @4032
4069 n3 exit_cs
4069 n3 -> n0 .mx_release @4085
4069 n3 -> n2 .mx_release @4086
4069 n3 -> n1 .mx_release @4087
4085 n0 enter_cs
4135 n0 exit_cs
4135 n0 -> n3 .mx_release @4148
4135 n0 -> n1 .mx_release @4150
4135 n0 -> n2 .mx_release @4153
4150 n1 enter_cs
4200 n1 exit_cs
4200 n1 -> n3 .mx_release @4213
4200 n1 -> n2 .mx_release @4214
4200 n1 -> n0 .mx_release @4214
4214 n2 enter_cs
4264 n2 exit_cs
4264 n2 -> n3 .mx_release @4274
4264 n2 -> n0 .mx_release @4275
4264 n2 -> n1 .mx_release @4283
4264 run end quiescent
