# ⚡ lin messenger (currently in development)

⚙️ This project has not reached 0.1 version, it is just the barebones of future messenger. Stay intact for alpha version!

💻 It must be fast, safe(secure), and convenient. So I'm building it using a microservice-like structure to make it scalable and easy to develop it further. It takes time.

---

✨ If you are willing to contribute to this project, and you've "vibecoded" some stuff, at least review it personally, please. I have no team to check every other code that does not work

🐋 NOTE: in every service there will be `docker-compose.yml`. I've added it to `.gitignore` because i have sensitive information in there. If you need these files for some reason, please open an issue. I will provide you with the structure of those files.

See the [prioritized improvement plan](IMPROVEMENTS.md) for current gaps, live integration results, and acceptance checks.

### Approximate progress on alpha(0.1) version:

- 🔐 lin_auth (Authentification service) - ![Progress](https://progress-bar.xyz/70)
- 🖧 lin_server (Actix websocket server) - ![Progress](https://progress-bar.xyz/32)
- 🔥 lin_client (Crossplatform Tauri client app) - ![Progress](https://progress-bar.xyz/0)

### Global TODOs:

- [x] Use UUIDs for user identities across auth and messaging (see [migration guide](USER_UUID_MIGRATION.md))
- [ ] Add chat membership and permissions(no arbitrary room joins)
- [ ] Save messages, add history and reconnect recovery
- [ ] Add refresh/logout and revoke active connections
- [ ] Set up HTTPS and browser Origin checks
- [ ] Add registration retries and RabbitMQ reconnect
- [ ] Add rate limits and bound queues/database work
- [ ] Add health checks and backup/restore checks
- [ ] Add secret-free deployment templates and track migrations
- [ ] Automate API and two-client chat tests
- [ ] Create CI/CD pipeline
- [ ] Build notification service(or look for completed solution)
- [ ] Build client side lmao
- [x] Chatroom creation
- [x] Token based access to user????
- [ ] Create connection with S3 storage

- [x] ~~db url in rust server is somewhat inaccesible, need to check it~~ It was due to bad loopback on a server parts

### Global notes and materials:

- [Chat App driven by WebSockets(could use some architecture patterns from this)](https://medium.com/@edhalliwell/chat-app-driven-by-websockets-using-socket-io-and-typescript-ed49611d6077)
- [e2ee one to many](https://medium.com/@TalBeerySec/the-ambassador-protocol-multi-device-e2ee-with-privacy-5c906a2d210a)
