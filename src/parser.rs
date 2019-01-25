use nom::line_ending;
use nom::float;
use nom::types::CompleteStr;
use na::{Matrix3, Rotation, Vector3};
use std::fs::File;
use std::io::{BufRead, BufReader};

pub struct SlamData {
    pub cameras: Vec<CameraWithPoints>,
    pub points: Vec<Vector3<f32>>,
}

#[derive(Debug)]
pub enum ParserError {
    IoError,
    IncompletePose,
    UnexpectedPixel,
}

pub struct CameraWithPoints {
    pub r_cw: Rotation<f32, na::U3>,
    pub t_cw: Vector3<f32>,
    pub pixels: Vec<[f32; 2]>,
}

pub struct Parser {
    s: ParserState,
    sd: SlamData,
}

#[derive(Clone)]
enum ParserState {
    StartPose(Vec<[f32; 4]>),
    Anything,
    PoseOrPoint, // after a 3d points, cannot get a pixel
}

impl Parser {
    named!(get_four_floats(nom::types::CompleteStr) ->(f32, f32, f32, f32), 
           ws!(
               do_parse!(
                   r1: float >>
                   r2: float >>
                   r3: float >>
                   t: float >>
                   opt!(complete!(line_ending)) >>
                   (r1, r2, r3, t)
                   )));

    named!(get_three_floats(nom::types::CompleteStr) ->(f32, f32, f32), 
           ws!(
           do_parse!(
               x: float >>
               y: float >>
               z: float >>
               opt!(complete!(line_ending)) >>
               (x, y, z)
               )));

    named!(get_two_floats(nom::types::CompleteStr) ->(f32, f32), 
           ws!(
           do_parse!(
               opt!(tag!("[")) >>
               xp: float >>
               opt!(tag!(",")) >>
               yp: float >>
               opt!(tag!("]")) >>
               opt!(complete!(line_ending)) >>
               (xp, yp)
               )));

    pub fn new() -> Parser {
        Parser {
            s: ParserState::PoseOrPoint,
            sd: SlamData{ cameras: Vec::new(), points: Vec::new(), }
        }
    }

    fn save_new_camera_pose(&mut self, pose: Vec<[f32; 4]>) {
        let mut r = Matrix3::zeros();
        let mut t = Vector3::zeros();
        for i in 0..3 {
            for j in 0..3 {
                r[(i, j)] = pose[i][j];
            }
            t[i] = pose[i][3];
        }
        self.sd.cameras.push(CameraWithPoints {
            r_cw: Rotation::from_matrix_unchecked(r),
            t_cw: t,
            pixels: Vec::new(),
        });
    }

    fn tuple2arr(t: (f32, f32, f32 ,f32)) -> [f32; 4] {
        [t.0, t.1, t.2, t.3]
    }

    fn try_four_floats(&mut self, l: &str) -> Result<Option<ParserState>, ParserError> {
        debug!("try_four_floats");
        let four_floats = Parser::get_four_floats(CompleteStr(l));
        let state = self.s.clone();
        match (four_floats, state) {
            // errors...
            (Err(_), ParserState::StartPose(_)) => Err(ParserError::IncompletePose),

            // create a new pose or append the new line
            (Ok((_, floats)), ParserState::Anything) |
                (Ok((_, floats)), ParserState::PoseOrPoint) => {
                    Ok(Some(ParserState::StartPose(vec![Parser::tuple2arr(floats)])))
                },
                (Ok((_, floats)), ParserState::StartPose(previous_lines)) => {
                    let mut previous_lines = previous_lines.clone();
                    previous_lines.push(Parser::tuple2arr(floats));
                    if previous_lines.len() == 3 {
                        self.save_new_camera_pose(previous_lines);
                        Ok(Some(ParserState::Anything))
                    } else {
                        Ok(Some(ParserState::StartPose(previous_lines)))
                    }
                }

            // this is not a pose
            (Err(_), ParserState::PoseOrPoint) |
                (Err(_), ParserState::Anything) => Ok(None)
        }
    }

    fn try_three_floats(&mut self, l: &str) -> Result<Option<ParserState>, ParserError> {
        debug!("try_three_floats");
        let three_floats = Parser::get_three_floats(CompleteStr(l));
        match (three_floats, &self.s) {
            // errors...
            (_, ParserState::StartPose(_)) => Err(ParserError::IncompletePose),

            // create a new pose or append the new line
            (Ok((_, floats)), ParserState::Anything) |
                (Ok((_, floats)), ParserState::PoseOrPoint) => {
                    self.sd.points.push(Vector3::new(floats.0, floats.1, floats.2));
                    Ok(Some(ParserState::PoseOrPoint))
                }

            // this is not a pose
            (Err(_), _) => Ok(None)
        }
    }

    fn try_two_floats(&mut self, l: &str) -> Result<Option<ParserState>, ParserError> {
        debug!("try_two_floats");
        let two_floats = Parser::get_two_floats(CompleteStr(l));
        match (two_floats, &mut self.s) {
            // errors...
            (Ok(_), ParserState::PoseOrPoint) => Err(ParserError::UnexpectedPixel),
            (_, ParserState::StartPose(_)) => Err(ParserError::IncompletePose),
            (Err(_), _) => Ok(None),

            // create a new pose or append the new line
            (Ok((_, floats)), ParserState::Anything) => {
                self.sd.cameras.last_mut().unwrap().pixels.push([floats.0, floats.1]);
                Ok(Some(ParserState::Anything))
            }

            // we just had a 3dp oint; a pixel doesn't make sense!
        }
    }

    pub fn next_line(&mut self, l: String) -> Result<(), ParserError> {
        if let Some(new_state) = self.try_four_floats(&l)? {
            self.s = new_state;
            return Ok(());
        }
        if let Some(new_state) = self.try_three_floats(&l)? {
            self.s = new_state;
            return Ok(());
        }

        if let Some(new_state) = self.try_two_floats(&l)? {
            self.s = new_state;
            return Ok(());
        }

        Ok(())
    }

    pub fn parse_file<S: Into<String>>(file_path: S) -> Result<SlamData, ParserError> {
        let file = File::open(file_path.into()).map_err(|_| ParserError::IoError)?;
        let mut parser = Parser::new();
        for line in BufReader::new(file).lines() {
            debug!("parsing line:");
            debug!("{:?}", line);
            parser.next_line(line.map_err(|_| ParserError::IoError)?)?;
        }
        Ok(parser.sd)
    }
}

#[cfg(test)]
mod tests {
    use ::parser::Parser;
    use nom::types::CompleteStr;
    #[test]
    fn failure_four_floats() {
        assert!(Parser::get_four_floats(CompleteStr("asd")).is_err());
            // how to make sure the whole string is consumed..?
        //assert!(Parser::get_four_floats(CompleteStr(" 1.0 1.0 1.0 1.0 1.0")).is_err());
        assert!(Parser::get_four_floats(CompleteStr(" 1.0 1.0 1.a 1.0")).is_err());
        assert!(Parser::get_four_floats(CompleteStr(" 1.0 1.0 1.0 --1.0")).is_err());
        assert!(Parser::get_four_floats(CompleteStr(" | 1.0 1.0 1.0 1.0")).is_err());
    }

    #[test]
    fn success_four_floats() {
        let values = vec![
            ("1.0 1.0 1.0 1.0",(1.0, 1.0, 1.0, 1.0)),
            ("\t123.0 -1.234 1.0 1.0 \t\t",(123.0, -1.234, 1.0, 1.0)),
            (" 1.0 1.0 1.0 1.0",(1.0, 1.0, 1.0, 1.0)),
            (" \t 1.0 1.0 1.0 -1.0",(1.0, 1.0, 1.0, -1.0)),
        ];

        for (line, floats) in values {
            let parse_res = Parser::get_four_floats(CompleteStr(line));
            assert!(parse_res.is_ok());
            assert_eq!(parse_res.unwrap().1, floats);
        }
    }

    #[test]
    fn success_two_floats() {
        let values = vec![
            ("1.0 1.0",(1.0, 1.0)),
            ("\t123.0 -1.234  \t\t",(123.0, -1.234)),
            (" 1.0 1.0 ",(1.0, 1.0)),
            (" \t 1.0 -1.0",(1.0, -1.0)),
        ];

        for (line, floats) in values {
            let parse_res = Parser::get_two_floats(CompleteStr(line));
            assert!(parse_res.is_ok());
            assert_eq!(parse_res.unwrap().1, floats);
        }
    }
}
