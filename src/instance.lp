starting_amount(usd, 10000).
pair(0, usd, btc, 5, 100000, 6000).
time(0..10).
final_amount(A) :- amount(10, btc, A).

0 { trade(T, P, BA, QA, Rn, Rd, A) } 1 :- pair(P, BA, QA, Rn, Rd, C), possible_amount(P, T, A), A <= C, time(T), amount(T, BA, C2), A <= C2, A > 0.

amount(T, Asset, A) :- amount(T - 1, Asset, A), not trade(T - 1, _, Asset, _, _, _, _), not trade(T - 1, _, _, Asset, _, _, _), time(T).
amount(T, Asset, A) :- amount(T - 1, Asset, A2), trade(T - 1, _, BA, Asset, Rn, Rd, A3), BA != Asset, A = A2 + (A3 * Rd / Rn), time(T).
amount(T, Asset, A) :- amount(T - 1, Asset, A2), trade(T - 1, _, Asset, QA, _, _, A3), QA != Asset, A = A2 - A3, time(T).

amount(0, Asset, 0) :- not starting_amount(Asset, _), pair(_, Asset, _, _, _, _).
amount(0, Asset, 0) :- not starting_amount(Asset, _), pair(_, _, Asset, _, _, _).
amount(0, Asset, A) :- starting_amount(Asset, A).

:- pair(_, A, A, _, _, _).

possible_amount(P, T, A) :- pair(P, _, _, _, _, A), amount(T, BA, C), A <= C.
possible_amount(P, T, A) :- pair(P, BA, _, _, _, C), amount(T, BA, A), A <= C.
possible_amount(P, T, A) :-
  pair(P, BA, QA, Rn, Rd, C),
  pair(P2, QA, _, _, _, _),
  P != P2,
  possible_amount(P2, T + 1, A2),
  A = A2 * Rn / Rd,
  A <= C.

:- trade(T1, P1, _, _, _, _, _), trade(T2, P2, _, _, _, _, _), T1 != T2, P1 = P2.
:- trade(_, P1, _, _, _, _, A1), trade(_, P2, _, _, _, _, A2), A1 != A2, P1 = P2.
:- trade(T1, P1, _, _, _, _, _), trade(T2, P2, _, _, _, _, _), T1 = T2, P1 != P2.
:- trade(T1, _, _, _, _, _, A1), trade(T2, _, _, _, _, _, A2), T1 = T2, A1 != A2.

#show final_amount/1.
#maximize { A@1: final_amount(A) }.