use std::collections::HashSet;
use std::net::IpAddr;

/// Rounds without a new responder before a route counts as stable.
pub const STABLE_ROUNDS: usize = 3;
/// Extra rounds granted to a route that never reaches its target.
pub const UNREACHED_GRACE_ROUNDS: usize = 5;

/// Tracks which (ttl, address) pairs have answered so a trace can stop once the path stops changing.
#[derive(Debug, Default)]
pub struct Convergence {
    seen: HashSet<(u8, IpAddr)>,
    rounds: usize,
    last_change: usize,
}

impl Convergence {
    /// Records the responders of the latest snapshot after `rounds` completed rounds.
    pub fn observe(&mut self, rounds: usize, responders: impl IntoIterator<Item = (u8, IpAddr)>) {
        if rounds <= self.rounds {
            return;
        }
        self.rounds = rounds;
        let mut changed = false;
        for responder in responders {
            changed |= self.seen.insert(responder);
        }
        if changed {
            self.last_change = rounds;
        }
    }

    /// A route is done when it ran `min_rounds`, saw no new responder for `STABLE_ROUNDS` rounds and
    /// either reached its target or used up the grace rounds for unreachable targets.
    pub const fn is_stable(&self, min_rounds: usize, reached: bool) -> bool {
        let quiet = self.rounds.saturating_sub(self.last_change) >= STABLE_ROUNDS;
        let enough = self.rounds >= min_rounds;
        let settled = reached || self.rounds >= min_rounds + UNREACHED_GRACE_ROUNDS;
        enough && quiet && settled
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn addr(last: u8) -> IpAddr {
        IpAddr::from([192, 0, 2, last])
    }

    #[test]
    fn waits_for_minimum_rounds_and_a_quiet_window() {
        let mut convergence = Convergence::default();
        for round in 1..=4 {
            convergence.observe(round, [(1, addr(1)), (2, addr(2))]);
        }
        assert!(!convergence.is_stable(5, true));
        convergence.observe(5, [(1, addr(1)), (2, addr(2))]);
        assert!(convergence.is_stable(5, true));
    }

    #[test]
    fn a_new_responder_restarts_the_quiet_window() {
        let mut convergence = Convergence::default();
        for round in 1..=5 {
            convergence.observe(round, [(1, addr(1))]);
        }
        convergence.observe(6, [(1, addr(1)), (2, addr(9))]);
        assert!(!convergence.is_stable(5, true));
        for round in 7..=9 {
            convergence.observe(round, [(1, addr(1)), (2, addr(9))]);
        }
        assert!(convergence.is_stable(5, true));
    }

    #[test]
    fn unreached_targets_get_grace_rounds() {
        let mut convergence = Convergence::default();
        for round in 1..=9 {
            convergence.observe(round, [(1, addr(1))]);
        }
        assert!(!convergence.is_stable(5, false));
        convergence.observe(10, [(1, addr(1))]);
        assert!(convergence.is_stable(5, false));
    }

    #[test]
    fn repeated_snapshots_of_the_same_round_are_ignored() {
        let mut convergence = Convergence::default();
        convergence.observe(3, [(1, addr(1))]);
        convergence.observe(3, [(2, addr(2))]);
        assert_eq!(convergence.rounds, 3);
        assert_eq!(convergence.seen.len(), 1);
    }
}
