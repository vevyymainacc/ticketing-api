import http from 'k6/http';
import { check } from 'k6';
import { Counter } from 'k6/metrics';

http.setResponseCallback(http.expectedStatuses(200, 409, 503));

const BASE_URL = __ENV.BASE_URL || 'http://localhost:8080';
const SEATS = parseInt(__ENV.SEATS || '5000', 10);
const VUS = parseInt(__ENV.VUS || '200', 10);
const ITERATIONS = parseInt(__ENV.ITERATIONS || '0', 10);

const booked = new Counter('res_booked');
const soldOut = new Counter('res_sold_out');
const shed = new Counter('res_shed');

const scenario =
  ITERATIONS > 0
    ? { executor: 'shared-iterations', vus: VUS, iterations: ITERATIONS, maxDuration: '120s' }
    : {
        executor: 'ramping-vus',
        startVUs: 0,
        stages: [
          { duration: '15s', target: 50 },
          { duration: '30s', target: VUS },
          { duration: '15s', target: 0 },
        ],
        gracefulStop: '5s',
      };

export const options = {
  scenarios: { main: scenario },
  thresholds: {
    http_req_duration: ['p(95)<500', 'p(99)<2000'],
    http_req_failed: ['rate<0.02'],
    checks: ['rate>0.99'],
  },
};

export function setup() {
  const res = http.post(
    `${BASE_URL}/events`,
    JSON.stringify({ name: 'loadtest', seat_count: SEATS }),
    { headers: { 'Content-Type': 'application/json' } },
  );
  check(res, { 'event created': (r) => r.status === 201 });
  return { eventId: res.json('event_id') };
}

export default function (data) {
  const key = `${__VU}-${__ITER}-${Date.now()}-${Math.random()}`;
  const res = http.post(
    `${BASE_URL}/events/${data.eventId}/reservations`,
    JSON.stringify({ user_ref: `vu-${__VU}` }),
    { headers: { 'Content-Type': 'application/json', 'Idempotency-Key': key } },
  );

  if (res.status === 200) booked.add(1);
  else if (res.status === 409) soldOut.add(1);
  else if (res.status === 503) shed.add(1);

  check(res, {
    'expected status': (r) =>
      r.status === 200 || r.status === 409 || r.status === 503,
  });
}
