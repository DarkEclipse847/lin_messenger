# ⚡ lin messenger (currently in development)

⚙️ This project has not reached 0.1 version, it is just the barebones of future messenger. Stay intact for alpha version!

💻 It must be fast, safe(secure), and convenient. So I'm building it using a microservice-like structure to make it scalable and easy to develop it further. It takes time.

---

✨ If you are willing to contribute to this project, and you've "vibecoded" some stuff, at least review it personally, please. I have no team to check every other code that does not work

🐋 NOTE: in every service there will be `docker-compose.yml`. I've added it to `.gitignore` because i have sensitive information in there. If you need these files for some reason, please open an issue. I will provide you with the structure of those files.

### Approximate progress on alpha(0.1) version:

- 🔐 lin_auth (Authentification service) - ![Progress](https://progress-bar.xyz/64)
- 🖧 lin_server (Actix websocket server) - ![Progress](https://progress-bar.xyz/30)
- 🔥 lin_client (Crossplatform Tauri client app) - ![Progress](https://progress-bar.xyz/0)

### Global TODOs:

- [ ] Consider changing ID generation logic to UUIDs for safety measures
- [ ] Add tests
- [ ] Create CI/CD pipeline after adding tests
- [ ] Build notification service(or look for completed solution)
- [ ] Build client side lmao
- [x] Chatroom creation
- [x] Token based access to user????
- [ ] Create connection with S3 storage

### Global notes and materials:

- [Chat App driven by WebSockets(could use some architecture patterns from this)](https://medium.com/@edhalliwell/chat-app-driven-by-websockets-using-socket-io-and-typescript-ed49611d6077)
- [e2ee one to many](https://medium.com/@TalBeerySec/the-ambassador-protocol-multi-device-e2ee-with-privacy-5c906a2d210a)
