extern crate timely;
extern crate differential_dataflow;

use std::sync::{Arc, Mutex};
use std::net::TcpListener;
use std::time::Duration;

use timely::dataflow::operators::Map;
use timely::logging::TimelyEvent;
use timely::communication::logging::{CommunicationEvent, CommunicationSetup};
use timely::dataflow::operators::Filter;
use timely::dataflow::operators::capture::{EventReader, Replay};
use differential_dataflow::AsCollection;
use differential_dataflow::operators::Consolidate;

fn main() {

    let mut args = ::std::env::args();
    args.next().unwrap();

    let work_peers: usize = args.next().expect("Must provide number of source peers").parse().expect("Source peers must be an unsigned integer");
    let comm_peers: usize = args.next().expect("Must provide number of source peers").parse().expect("Comms peers must be an unsigned integer");
    let granularity: u64  = args.next().expect("Must provide report granularity in seconds").parse().expect("Granularity must be an unsigned integer");

    println!("starting with work peers: {}, comm peers: {}, granularity: {}", work_peers, comm_peers, granularity);

    let t_listener = TcpListener::bind("0.0.0.0:8000").unwrap();
    let d_listener = TcpListener::bind("0.0.0.0:9000").unwrap();
    let t_sockets =
    Arc::new(Mutex::new((0..work_peers).map(|_| {
            let socket = t_listener.incoming().next().unwrap().unwrap();
            socket.set_nonblocking(true).expect("failed to set nonblocking");
            Some(socket)
        }).collect::<Vec<_>>()));
    let d_sockets =
    Arc::new(Mutex::new((0..comm_peers).map(|_| {
            let socket = d_listener.incoming().next().unwrap().unwrap();
            socket.set_nonblocking(true).expect("failed to set nonblocking");
            Some(socket)
        }).collect::<Vec<_>>()));

    timely::execute_from_args(std::env::args(), move |worker| {

        let index = worker.index();
        let peers = worker.peers();

        let t_streams =
        t_sockets
            .lock()
            .unwrap()
            .iter_mut()
            .enumerate()
            .filter(|(i, _)| *i % peers == index)
            .map(move |(_, s)| s.take().unwrap())
            .map(|r| EventReader::<Duration, (Duration, usize, TimelyEvent),_>::new(r))
            .collect::<Vec<_>>();

        let d_streams =
        d_sockets
            .lock()
            .unwrap()
            .iter_mut()
            .enumerate()
            .filter(|(i, _)| *i % peers == index)
            .map(move |(_, s)| s.take().unwrap())
            .map(|r| EventReader::<Duration, (Duration, CommunicationSetup, CommunicationEvent),_>::new(r))
            .collect::<Vec<_>>();

        worker.dataflow::<_,_,_>(|scope| {

            let t_events = t_streams.replay_into(scope);
            let d_events = d_streams.replay_into(scope);

            // println!("SIZEOF: {}", ::std::mem::size_of::<(Duration, usize, TimelyEvent)>());

            t_events
                .flat_map(move |(ts, _worker, datum)| {
                    let ts = Duration::from_secs((ts.as_secs()/granularity + 1) * granularity);
                    Some((datum.clone(), ts, ::std::mem::size_of_val(&datum) as isize))
                })
                .as_collection()
                .inspect(|x| println!("WORKER\t{:?}", x));
            ;

            d_events
                .flat_map(move |(ts, _worker, datum)| {
                    let ts = Duration::from_secs((ts.as_secs()/granularity + 1) * granularity);
                    Some((datum, ts, ::std::mem::size_of_val(&datum) as isize))
                })
                .as_collection()
                .inspect(|x| println!("COMM_CHANNEL\t{:?}", x));
            ;

        });

    }).unwrap(); // asserts error-free execution
}
