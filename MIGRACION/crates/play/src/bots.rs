//! Players with no game of their own: they say what they ask of each step by a script and nothing
//! else (no prediction, no digest to compare), to load a server with as many as wanted and measure
//! what its steps cost (`docs/PLAN_AUTORITATIVO.md` fase 3). Their clock is the server's, read off
//! its snapshots, plus a few steps.
use crate::{
    net::{self, Cmd, REPEAT, SNAP, Snap},
    pilot::Input,
};
use lunar_net::{Client, Event, Reader};

/// What a bot does, by the step: walk round and jump, run with the pack on, stand.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Script {
    Walk,
    Jets,
    Stand,
}

pub struct Bot {
    pub client: Client,
    pub script: Script,
    /// The step of our next command (none until the server's first snapshot).
    step: Option<u64>,
    due: f64,
    cmds: [Cmd; REPEAT],
    out: Vec<u8>,
    snap: Snap,
    seed: u64,
}

/// Steps ahead of the server's newest snapshot a bot's commands are for.
const AHEAD: u64 = 12;

impl Bot {
    pub fn new(client: Client, script: Script, seed: u64) -> Bot {
        Bot { client, script, step: None, due: 0.0, cmds: [Cmd::default(); REPEAT], out: Vec::new(), snap: Snap::default(), seed }
    }

    /// What came, read (only the snapshots' step matters), and the commands of the steps due over
    /// `dt` s, sent.
    pub fn update(&mut self, now: f64, dt: f64) {
        self.client.update(now);
        for e in self.client.events() {
            if let Event::Game { reliable: false, data } = e {
                let mut r = Reader::new(&data);
                if r.u8() == Ok(SNAP) && net::read_snap(&mut r, &mut self.snap).is_ok() && self.step.is_none() {
                    self.step = Some(self.snap.step + AHEAD);
                }
            }
        }
        let Some(mut step) = self.step else { return };
        self.due += dt;
        while self.due >= crate::game::STEP {
            self.due -= crate::game::STEP;
            let c = self.ask(step);
            self.cmds.rotate_right(1);
            self.cmds[0] = c;
            let n = (self.cmds.iter().filter(|c| c.step + REPEAT as u64 > step && c.step <= step).count()).max(1);
            net::write_cmds(&self.cmds[..n], None, &mut self.out);
            self.client.send_quick(&self.out);
            step += 1;
        }
        self.step = Some(step);
    }

    /// What the script asks of step `n`.
    fn ask(&mut self, n: u64) -> Cmd {
        let phase = (n + self.seed * 97) % 600;
        let mut i = Input::default();
        let mut c = Cmd { step: n, steady: true, ..Cmd::default() };
        match self.script {
            Script::Walk => {
                i.forward = 1.0;
                i.run = phase > 300;
                i.jump = phase % 150 == 0;
                c.yaw = (n as f64 * 0.01 + self.seed as f64).rem_euclid(std::f64::consts::TAU);
            }
            Script::Jets => {
                c.pack = true;
                i.forward = 1.0;
                i.vertical = if phase < 200 { 1.0 } else { 0.0 };
                i.jump = phase == 10;
                c.yaw = (n as f64 * 0.004 + self.seed as f64).rem_euclid(std::f64::consts::TAU);
            }
            Script::Stand => {}
        }
        c.input = i;
        c
    }
}
