So, i think it is time to create a username resolution convention
Here what i think it should look like
Maybe i need swagger docs or some shit

lin.wtf/ <--- starting page, introdusing to project connection through https
lin.wtf/registration <--- page for registration
lin.wtf/login <--- login page
lin.wtf/{username} <--- logged in users see this, e.g.(chats, menu, navbar, all that crap). But how does we prevent unauthorised user to see this? Is JWT login capable of that? I forgor
lin.wtf/{username}/profile <--- info about one user
lin.wtf/{id} <--- chatroom
